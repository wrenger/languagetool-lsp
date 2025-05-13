# LanguageTool LSP

This is a Language Server Protocol (LSP) client for [LanguageTool](https://languagetool.org/), a powerful grammar and style checker.
This extension integrates LanguageTool into Zed, providing real-time feedback to Markdown, LaTeX, Typst, or Plaintext documents.

Warning: This extension is currently in development and may not work as expected.

## Manual Installation

Download the repo:

```sh
git@github.com:wrenger/languagetool-lsp.git
```

Install it with the `zed: install dev extension` command in zed.
Or in the Extension tab.
Choose the `zed` subdirectory.

## Language Server Installation

The extension **automatically** installs the language server for Linux/x86_64, macOS/x86_64, and macOS/aarch64.

All other platforms have to build it manually (`cargo b -r`).
And then add the following Zed configuration:

```json
{
  "lsp": {
    "languagetool-lsp": {
      "initialization_options": {},
      "binary": {
        "path": "<path/to>/languagetool-lsp/target/release/languagetool-lsp",
        "args": []
      }
    }
  }
}
```
