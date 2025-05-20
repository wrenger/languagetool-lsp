use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use api::Match;
use changes::Changes;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_lsp_server::lsp_types::{
    self, CodeAction, CodeActionKind, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, Diagnostic, DiagnosticOptions, DiagnosticServerCapabilities,
    DiagnosticSeverity, DidChangeConfigurationParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    ExecuteCommandOptions, ExecuteCommandParams, InitializeParams, InitializeResult, MessageType,
    Range as DocRange, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server, jsonrpc};
use tracing::{error, info, warn};

mod annotated;
mod api;
mod changes;
mod settings;
mod source;
mod util;

use annotated::plaintext;
use settings::Settings;
use source::SourceFile;
use util::RangeExt;

struct Backend {
    client: Client,
    settings: RwLock<Settings>,
    /// Currently open documents
    documents: RwLock<HashMap<Uri, Document>>,
    dictionary: RwLock<HashSet<String>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        info!("Init {:?}", params.initialization_options);
        info!("{:?}", params.capabilities.general);
        info!(
            "{:?}",
            params.capabilities.text_document.and_then(|d| d.diagnostic)
        );
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("languagetool-lsp".to_string()),
                        ..Default::default()
                    },
                )),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "languagetool-lsp.check".to_string(),
                        "languagetool-lsp.synonyms".to_string(),
                        "languagetool-lsp.ignore".to_string(),
                        "languagetool-lsp.words-add".to_string(),
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "LanguageTool LSP".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Settings: {:?}", params.settings);
        *self.settings.write().await = serde_json::from_value(params.settings).unwrap();
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!(
            "DidOpen: {} {}",
            params.text_document.version,
            params.text_document.uri.as_str()
        );

        self.documents.write().await.insert(
            params.text_document.uri,
            Document::new(
                SourceFile::new(params.text_document.text),
                Some(params.text_document.version),
            ),
        );
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!(
            "DidChange: {} {}",
            params.text_document.version,
            params.text_document.uri.as_str()
        );

        let mut open_docs = self.documents.write().await;
        let Some(doc) = open_docs.get_mut(&params.text_document.uri) else {
            return;
        };

        for change in params.content_changes {
            if let Some(range) = change.range {
                doc.changed_lines.add_change(
                    range.start.line as usize..range.end.line as usize + 1,
                    change.text.split('\n').count(),
                );

                let start = doc.source.to_offset(range.start).unwrap();
                let end = doc.source.to_offset(range.end).unwrap();

                doc.source.replace(start..end, &change.text);
                doc.version = Some(params.text_document.version);

                // Update positions for matches behind the change
                let shift = change.text.len() as isize - (end as isize - start as isize);
                for m in &mut doc.matches {
                    if m.range.start >= end {
                        m.range.start = (m.range.start as isize + shift) as usize;
                    }
                    if m.range.end >= end {
                        m.range.end = (m.range.end as isize + shift) as usize;
                    }
                }
            } else {
                // No range means replace the whole document
                doc.source = SourceFile::new(change.text);
                doc.version = Some(params.text_document.version);
                doc.matches.clear();
                doc.changed_lines.clear();
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let DidSaveTextDocumentParams {
            text_document,
            text,
        } = params;

        info!("DidSave: {}", text_document.uri.as_str());

        let mut open_docs = self.documents.write().await;
        let Some(doc) = open_docs.get_mut(&text_document.uri) else {
            return;
        };

        if let Some(text) = text {
            if text != doc.source.text() {
                warn!("Document has dirty changes! {}", text_document.uri.as_str());
                doc.source = SourceFile::new(text);
                doc.changed_lines
                    .add_change(0..doc.source.lines().len(), doc.source.lines().len());
            }
        };

        if let Err(err) = self.update_matches(doc).await {
            error!("Failed diagnostics: {err}\n{}", err.backtrace());
            self.client
                .show_message(MessageType::ERROR, format!("{err}"))
                .await;
        } else {
            self.show_diagnostics(&text_document.uri, doc).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("DidClose: {}", params.text_document.uri.as_str());

        // Clear diagnostics for the closed document
        self.client
            .publish_diagnostics(params.text_document.uri.clone(), Vec::new(), None)
            .await;

        let mut open_docs = self.documents.write().await;
        open_docs.remove(&params.text_document.uri);
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        info!("Shutdown");
        Ok(())
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> jsonrpc::Result<Option<CodeActionResponse>> {
        let open_docs = self.documents.read().await;
        let Some(doc) = open_docs.get(&params.text_document.uri) else {
            return Ok(None);
        };

        let mut actions = Vec::new();

        let lt_diags = params
            .context
            .diagnostics
            .iter()
            .filter(|d| d.source == Some("languagetool-lsp".into()))
            .cloned()
            .collect::<Vec<_>>();

        for diag in &lt_diags {
            // Replacements
            if let Some(data) = &diag.data {
                let data: Vec<String> = serde_json::from_value(data.clone()).unwrap();
                for replacement in data {
                    actions.push(CodeAction {
                        title: format!("{replacement:?}"),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(WorkspaceEdit {
                            changes: Some(
                                [(
                                    params.text_document.uri.clone(),
                                    vec![TextEdit {
                                        range: diag.range,
                                        new_text: replacement,
                                    }],
                                )]
                                .into(),
                            ),
                            ..Default::default()
                        }),
                        diagnostics: Some(vec![diag.clone()]),
                        ..Default::default()
                    });
                }
            }

            // Add to dictionary
            if diag.severity == Some(DiagnosticSeverity::WARNING) {
                if let (Some(start), Some(end)) = (
                    doc.source.to_offset(diag.range.start),
                    doc.source.to_offset(diag.range.end),
                ) {
                    info!("Add to dictionary {start}..{end}");
                    let selection = &doc.source.text()[start..end];
                    actions.push(CodeAction {
                        title: format!("Add {selection:?} to Dictionary"),
                        kind: Some(CodeActionKind::QUICKFIX),
                        command: Some(lsp_types::Command {
                            title: "Add to Dictionary".to_string(),
                            command: "languagetool-lsp.words-add".to_string(),
                            arguments: Some(vec![
                                serde_json::to_value(LTCommandParams {
                                    text_document: params.text_document.clone(),
                                    range: diag.range,
                                })
                                .unwrap(),
                            ]),
                        }),
                        diagnostics: Some(vec![diag.clone()]),
                        ..Default::default()
                    });
                }
            }
        }

        // Ignore diagnostics
        if !lt_diags.is_empty() {
            actions.push(CodeAction {
                title: "Ignore Lints".to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                command: Some(lsp_types::Command {
                    title: "Ignore Lints".to_string(),
                    command: "languagetool-lsp.ignore".to_string(),
                    arguments: Some(vec![
                        serde_json::to_value(LTCommandParams {
                            text_document: params.text_document.clone(),
                            range: params.range,
                        })
                        .unwrap(),
                    ]),
                }),
                diagnostics: Some(lt_diags.clone()),
                ..Default::default()
            })
        }

        // Check spelling
        actions.push(CodeAction {
            title: "Check Spelling".to_string(),
            kind: Some(CodeActionKind::SOURCE),
            command: Some(lsp_types::Command {
                title: "Check Spelling".to_string(),
                command: "languagetool-lsp.check".to_string(),
                arguments: Some(vec![
                    serde_json::to_value(LTCommandParams {
                        text_document: params.text_document.clone(),
                        range: params.range,
                    })
                    .unwrap(),
                ]),
            }),
            ..Default::default()
        });

        // Synonyms
        if let (Some(start), Some(end)) = (
            doc.source.to_offset(params.range.start),
            doc.source.to_offset(params.range.end),
        ) {
            let selection = doc.source.text()[start..end].trim();
            if !selection.is_empty() && !selection.contains(char::is_whitespace) {
                info!("add synonyms {start}..{end} {selection:?}");
                actions.push(CodeAction {
                    title: format!("Synonyms for {selection:?}"),
                    kind: Some(CodeActionKind::SOURCE),
                    command: Some(lsp_types::Command {
                        title: "Synonyms".to_string(),
                        command: "languagetool-lsp.synonyms".to_string(),
                        arguments: Some(vec![
                            serde_json::to_value(LTCommandParams {
                                text_document: params.text_document.clone(),
                                range: params.range,
                            })
                            .unwrap(),
                        ]),
                    }),
                    ..Default::default()
                });
            }
        }

        Ok((!actions.is_empty()).then_some(actions.into_iter().map(|a| a.into()).collect()))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> jsonrpc::Result<Option<lsp_types::LSPAny>> {
        info!("ExecuteCommand: {:?}", params.command);
        let ExecuteCommandParams {
            command,
            mut arguments,
            ..
        } = params;

        if arguments.len() != 1 {
            error!("Invalid arguments: {arguments:?}");
            return Err(jsonrpc::Error::invalid_params(
                "Invalid number of arguments".to_string(),
            ));
        }

        let first = arguments.remove(0);
        let params = serde_json::from_value::<LTCommandParams>(first)
            .map_err(|e| jsonrpc::Error::invalid_params(format!("Invalid params: {e}")))?;

        let mut open_docs = self.documents.write().await;
        let Some(doc) = open_docs.get_mut(&params.text_document.uri) else {
            error!("No document found: {}", params.text_document.uri.as_str());
            return Ok(None);
        };

        let res = match command.as_str() {
            "languagetool-lsp.check" => self.command_check(params.range, doc).await,
            "languagetool-lsp.synonyms" => self.command_synonyms(params.range, doc).await,
            "languagetool-lsp.ignore" => self.command_ignore(params.range, doc).await,
            "languagetool-lsp.words-add" => self.command_words_add(params.range, doc).await,
            _ => {
                error!("Unknown command: {command:?}");
                return Err(jsonrpc::Error::method_not_found());
            }
        };

        if let Err(err) = res {
            error!("Command failed: {err}\n{}", err.backtrace());
            self.client
                .show_message(MessageType::ERROR, format!("{err}"))
                .await;
            return Err(jsonrpc::Error::internal_error());
        } else {
            self.show_diagnostics(&params.text_document.uri, doc).await;
        }

        Ok(None)
    }
}

#[derive(Serialize, Deserialize)]
struct LTCommandParams {
    text_document: lsp_types::TextDocumentIdentifier,
    range: lsp_types::Range,
}

impl Backend {
    async fn show_diagnostics(&self, uri: &Uri, doc: &mut Document) {
        let diags = doc.diagnostics();
        self.client
            .publish_diagnostics(uri.clone(), diags, doc.version)
            .await
    }

    async fn update_matches(&self, doc: &mut Document) -> Result<()> {
        let changes = doc.changed_lines.changes().clone();
        doc.changed_lines.clear();

        for lines in changes {
            info!("Check lines: {lines:?}");

            // TODO: Parse markdown/latex/typst
            let (mut range, mut annot) = plaintext::annotate(&doc.source, lines)?;
            range.start += annot.optimize();
            if annot.len() == 0 {
                info!("Skip empty annotation");
                continue;
            }

            info!("Check {range:?} ({})", annot.len());
            let settings = self.settings.read().await.clone();
            let mut matches = api::check(annot, range.start, &settings, None).await?;
            info!("Matches: {}", matches.len());

            for m in &matches {
                info!(
                    "Match: {} {} {}: {:?}\n-> {:?}",
                    m.range.start,
                    m.range.end,
                    m.title,
                    &doc.source.text()[m.range.clone()],
                    &m.replacements
                );
            }

            // Remove spelling matches part of the dictionary
            if !settings.sync_dictionary {
                let dict = self.dictionary.read().await;
                matches = matches
                    .into_iter()
                    .filter(|m| {
                        !(m.category == "TYPOS"
                            && dict.contains(&doc.source.text()[m.range.clone()]))
                    })
                    .collect();
            }

            // Remove matches that overlap with the changed lines
            doc.matches.retain(|m| !m.range.touches(&range));
            doc.matches.append(&mut matches);
            doc.matches.sort_by_key(|m| m.range.start);
        }

        Ok(())
    }

    async fn command_check(&self, range: lsp_types::Range, doc: &mut Document) -> Result<()> {
        doc.changed_lines.add_change(
            range.start.line as usize..range.end.line as usize + 1,
            range.end.line as usize - range.start.line as usize + 1,
        );
        self.update_matches(doc).await
    }

    async fn command_synonyms(&self, range: lsp_types::Range, doc: &mut Document) -> Result<()> {
        let (Some(start), Some(end)) = (
            doc.source.to_offset(range.start),
            doc.source.to_offset(range.end),
        ) else {
            return Err(anyhow!("Invalid range: {:?}", range));
        };
        info!("Synonyms for {:?}", start..end);

        let Some(((pos, _), line)) = doc
            .source
            .line_range(range.start.line as usize..range.end.line as usize + 1)
        else {
            return Err(anyhow!("Invalid range: {:?}", start..end));
        };

        let synonyms = self
            .settings
            .read()
            .await
            .synonyms
            .query(line, start - pos.byte..end - pos.byte)
            .await
            .map_err(|e| jsonrpc::Error::invalid_params(format!("Synonyms: {e}")))?;

        doc.matches.push(Match {
            range: start..end,
            title: "Synonyms".to_string(),
            message: String::new(),
            category: "SYNONYMS".to_string(),
            rule: "SYNONYMS".to_string(),
            replacements: synonyms,
        });
        Ok(())
    }

    async fn command_ignore(&self, range: lsp_types::Range, doc: &mut Document) -> Result<()> {
        let (Some(start), Some(end)) = (
            doc.source.to_offset(range.start),
            doc.source.to_offset(range.end),
        ) else {
            return Err(anyhow!("Invalid range: {:?}", range));
        };
        info!("ignore {start}..{end}");
        doc.matches.retain(|m| !m.range.touches(&(start..end)));
        Ok(())
    }

    async fn command_words_add(&self, range: lsp_types::Range, doc: &mut Document) -> Result<()> {
        let (Some(start), Some(end)) = (
            doc.source.to_offset(range.start),
            doc.source.to_offset(range.end),
        ) else {
            return Err(anyhow!("Invalid range: {:?}", range));
        };
        let Some(word) = doc.source.text().get(start..end) else {
            return Err(anyhow!("Invalid range: {:?}", range));
        };
        info!("add word {word:?}");
        let settings = self.settings.read().await.clone();

        if settings.sync_dictionary && (settings.username.is_empty() || settings.api_key.is_empty())
        {
            self.client
                .show_message(
                    MessageType::WARNING,
                    "Syncing words is only supported for premium users",
                )
                .await;
        }

        if settings.sync_dictionary && !settings.username.is_empty() && !settings.api_key.is_empty()
        {
            info!("Add {word:?} to remote dict");
            api::words::add(&settings, word).await?;
            self.client
                .show_message(
                    MessageType::INFO,
                    format!("Added {word:?} to remote dictionary"),
                )
                .await;
        } else {
            info!("Add {word:?} to local dict");
            self.dictionary.write().await.insert(word.to_string());
            self.client
                .show_message(
                    MessageType::INFO,
                    format!("Added {word:?} to local dictionary"),
                )
                .await;
        }

        // Remove corresponding matches
        doc.matches
            .retain(|m| !(m.category == "TYPOS" && word == &doc.source.text()[m.range.clone()]));
        Ok(())
    }
}

struct Document {
    source: SourceFile,
    version: Option<i32>,
    matches: Vec<Match>,
    changed_lines: Changes,
}
impl Document {
    fn new(source: SourceFile, version: Option<i32>) -> Self {
        let mut changed_lines = Changes::new();
        // Initially everyting is changed
        changed_lines.add_change(0..source.lines().len(), source.lines().len());
        Self {
            source,
            version,
            matches: Vec::new(),
            changed_lines,
        }
    }
    fn diagnostics(&self) -> Vec<Diagnostic> {
        self.matches
            .iter()
            .map(|m| Diagnostic {
                range: DocRange {
                    start: self.source.to_position(m.range.start).unwrap(),
                    end: self.source.to_position(m.range.end).unwrap(),
                },
                data: Some(m.replacements.clone().into()),
                message: format!(
                    "{}\n\n{}\n{} > {}\n",
                    m.title, m.message, m.category, m.rule
                ),
                severity: Some(match m.category.as_str() {
                    "COLLOQUIALISMS" | "REDUNDANCY" | "STYLE" | "SYNONYMS" => {
                        DiagnosticSeverity::HINT
                    }
                    "TYPOS" => DiagnosticSeverity::WARNING,
                    _ => DiagnosticSeverity::INFORMATION,
                }),
                source: Some("languagetool-lsp".into()),
                ..Default::default()
            })
            .collect()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .init();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        settings: Default::default(),
        documents: Default::default(),
        dictionary: Default::default(),
    });

    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
