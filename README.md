# repoyank

`repoyank` is a CLI tool specifically designed to help you interactively select and format code snippets from your repository, perfect for easily preparing structured input for large language models (LLMs) without the need to upload your sensitive data to third-party services.

## 🚀 Key Benefits

- **Privacy-first:** Keep your codebase secure by preparing snippets locally instead of uploading to external tools.
- **LLM-friendly output:** Neatly formatted for direct copy-paste into GPT or other language models.
- **Interactive Tree View:** Easily navigate your repository and precisely select files and directories.

## 🎯 Features

- **Interactive Selection:** Intuitive tree-view interface for selecting files or directories.
- **Recursive File Inclusion:** Automatically gathers nested files.
- **Structured Clipboard Output:** Provides well-formatted snippets with clear file separation.
- **Customizable File Filtering:** Easily include or exclude specific file types.
- **Clipboard Integration:** Works smoothly across Linux (Wayland/X11), macOS, and Windows via `arboard`.
- **Git-aware:** Optional inclusion of files normally ignored by `.gitignore`.

## 📥 Installation

Make sure [Rust](https://rustup.rs) is installed, then run:

```bash
cargo install --git https://github.com/TimKoornstra/repoyanker.git --branch main
````

## 🛠 Usage

```bash
repoyank [OPTIONS] [DIR]
```

### Arguments

* `DIR`: Root directory (default: current directory `.`)

### Options

| Flag                      | Description                                              |
| ------------------------- | -------------------------------------------------------- |
| `--types <ext1,ext2,...>` | Filter by comma-separated file extensions (without dots) |
| `--include-ignored`       | Include files ignored by `.gitignore`                    |
| `-h`, `--help`            | Display help information                                 |
| `-V`, `--version`         | Display version information                              |

### Example

Interactive selection tailored for Rust and Markdown files:

```bash
repoyank --types rs,md
```

After selection, your clipboard will contain output like:

```
./
├─ src/
│  └─ main.rs
└─ README.md

---
File: src/main.rs
---
// File contents

---
File: README.md
---
# Project README
...
```

And provide you a helpful confirmation:

```
✅ Copied 2 files (≈ 150 tokens) to the clipboard.
```

## 💻 Development

Clone and run locally:

```bash
git clone https://github.com/TimKoornstra/repoyanker.git
cd repoyanker
cargo run -- --types rs,md
```

## 🤝 Contributing

This is my first Rust project! Contributions, suggestions, and improvements are very welcome. Feel free to open issues or pull requests at [GitHub](https://github.com/TimKoornstra/repoyanker).

## 📄 License

This project is licensed under the GPLv3 License. See [LICENSE](LICENSE) for details.

