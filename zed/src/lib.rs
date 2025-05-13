use std::fs;

use zed::settings::LspSettings;
use zed_extension_api::{self as zed, serde_json};

const NAME: &str = "languagetool-lsp";

struct Extension {
    cached_binary: Option<String>,
}

impl zed::Extension for Extension {
    fn new() -> Self {
        Self {
            cached_binary: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        println!("Language server command called for {language_server_id}");

        let settings = LspSettings::for_worktree(NAME, worktree);
        println!("Settings: {settings:?}");

        let binary_settings = settings.ok().and_then(|lsp_settings| lsp_settings.binary);
        let args = binary_settings
            .as_ref()
            .and_then(|settings| settings.arguments.clone())
            .unwrap_or_default();

        if let Some(path) = binary_settings.and_then(|settings| settings.path) {
            return Ok(zed::Command {
                command: path,
                args,
                env: vec![],
            });
        }
        if let Some(path) = worktree.which(NAME) {
            return Ok(zed::Command {
                command: path,
                args,
                env: vec![],
            });
        }
        if let Some(path) = self.cached_binary.as_ref() {
            return Ok(zed::Command {
                command: path.clone(),
                args,
                env: vec![],
            });
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            &format!("wrenger/{NAME}"),
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "{NAME}-{arch}-{target}.zip",
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            target = match platform {
                zed::Os::Mac => "apple-darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("{NAME}-{}", release.version);
        let binary_path = if platform == zed::Os::Windows {
            format!("{version_dir}/{NAME}.exe")
        } else {
            format!("{version_dir}/{NAME}")
        };

        if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::Zip,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)
                .map_err(|e| format!("failed to make lsp executable {e}"))?;

            // Cleanup old versions
            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(entry.path()).ok();
                }
            }
        }

        self.cached_binary = Some(binary_path.clone());
        Ok(zed::Command {
            command: binary_path,
            args,
            env: vec![],
        })
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<Option<serde_json::Value>> {
        println!("Workspace configuration called for {language_server_id}");
        let lsp_settings = LspSettings::for_worktree(NAME, worktree)?;
        Ok(lsp_settings.settings)
    }
}

zed::register_extension!(Extension);
