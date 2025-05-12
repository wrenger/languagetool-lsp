use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::RwLock;
use tower_lsp_server::lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    Diagnostic, DiagnosticServerCapabilities, DiagnosticSeverity, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, InitializeParams, InitializeResult, MessageType, Range,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Uri, WorkspaceEdit,
};
use tower_lsp_server::{Client, LanguageServer, LspService, Server, jsonrpc};
use tracing::{error, info};

mod annotated;
mod api;
mod settings;
mod source;
mod util;

use annotated::AnnotatedText;
use settings::Settings;
use source::SourceFile;

struct Backend {
    client: Client,
    settings: RwLock<Settings>,
    open_docs: RwLock<HashMap<Uri, SourceFile>>,
}

impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        info!("Init {:?}", params.initialization_options);
        info!("{:?}", params.capabilities.general);
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                diagnostic_provider: Some(
                    DiagnosticServerCapabilities::Options(Default::default()),
                ),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "LanguageTool LSP".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        info!("Config: {:#?}", params.settings);
        *self.settings.write().await = serde_json::from_value(params.settings).unwrap();
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        info!(
            "DidOpen: {} {}",
            params.text_document.version,
            params.text_document.uri.as_str()
        );

        self.open_docs.write().await.insert(
            params.text_document.uri.clone(),
            SourceFile::new(
                params.text_document.text.clone(),
                params.text_document.version,
            ),
        );
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        info!(
            "DidChange: {} {}",
            params.text_document.version,
            params.text_document.uri.as_str()
        );

        let mut open_docs = self.open_docs.write().await;
        let doc = open_docs.get_mut(&params.text_document.uri).unwrap();
        for change in params.content_changes {
            assert!(change.range.is_none());
            *doc = SourceFile::new(change.text.clone(), params.text_document.version);
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let DidSaveTextDocumentParams {
            text_document,
            text,
        } = params;

        info!("DidSave: {}", text_document.uri.as_str());

        let mut open_docs = self.open_docs.write().await;
        let Some(doc) = open_docs.get_mut(&text_document.uri) else {
            error!("No document found: {}", text_document.uri.as_str());
            return;
        };
        if let Some(text) = text {
            *doc = SourceFile::new(text.clone(), doc.version());
        };
        self.update_diagnostics(&text_document.uri, doc).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        info!("DidClose: {}", params.text_document.uri.as_str());

        // Clear diagnostics for the closed document
        self.client
            .publish_diagnostics(params.text_document.uri.clone(), Vec::new(), None)
            .await;

        let mut open_docs = self.open_docs.write().await;
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
        let mut actions = Vec::new();
        for diag in params.context.diagnostics {
            if diag.source != Some("languagetool-lsp".into()) {
                continue;
            }

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
        }
        Ok((!actions.is_empty()).then_some(actions.into_iter().map(|a| a.into()).collect()))
    }
}

impl Backend {
    async fn update_diagnostics(&self, uri: &Uri, source: &SourceFile) {
        match self.collect_diagnostics(source).await {
            Ok(diagnostics) => {
                self.client
                    .publish_diagnostics(uri.clone(), diagnostics, Some(source.version()))
                    .await
            }
            Err(err) => {
                error!("Failed diagnostics: {err}");
                self.client
                    .show_message(MessageType::ERROR, format!("{err}"))
                    .await;
            }
        }
    }

    async fn collect_diagnostics(&self, source: &SourceFile) -> Result<Vec<Diagnostic>> {
        let mut annot = AnnotatedText::new();
        // TODO: Parse markdown/latex/typst
        annot.add_text(source.text().into());
        annot.optimize();

        info!("Check: {:?}", source.text().len());
        let settings = self.settings.read().await.clone();
        // TODO: Only check the changed range
        let matches = api::check(annot, 0, &settings, None).await?;
        info!("Matches: {}", matches.len());

        for m in &matches {
            info!(
                "Match: {} {} {:?} {}",
                m.range.start,
                m.range.end,
                &source.text()[m.range.clone()],
                m.title
            );
        }

        let diagnostics = matches
            .into_iter()
            .map(|m| Diagnostic {
                range: Range {
                    start: source.to_position(m.range.start).unwrap(),
                    end: source.to_position(m.range.end).unwrap(),
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
            .collect();

        Ok(diagnostics)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .init();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        settings: Default::default(),
        open_docs: Default::default(),
    });

    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
