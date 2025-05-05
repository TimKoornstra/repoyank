# repoyank

`repoyank` is a command-line utility for interactively browsing your repository, selecting files and directories, and yanking a structured code snippet summary to your clipboard. This tool is perfect for quickly copying snippets from multiple files in a project, with an estimated token count for GPT-style workflows.

> **Status:** Work-in-progress (WIP)

---

## Features

* **Interactive tree view** of your repository.
* **Multi-select interface** to pick files and directories.
* **Recursive selection** when choosing directories.
* **Structured output** including a tree diagram and file contents, separated with headings.
* **Clipboard integration**: copies output to your system clipboard (supports Linux daemon, macOS, and Windows via `arboard`).
* **File type filtering** via `--types` flag.
* **Gitignore handling** with `--include-ignored`.

## Installation

Ensure you have [Rust](https://rust-lang.org) and `cargo` installed.

```bash
cargo install --git https://github.com/TimKoornstra/repoyank.git --branch main
```

## Usage

```bash
repoyank [OPTIONS] [DIR]
```

### Arguments

* `DIR` – Root directory to scan (defaults to current working directory `.`).

### Options

| Flag                      | Description                                          |
| ------------------------- | ---------------------------------------------------- |
| `--types <EXT1,EXT2,...>` | Comma-separated file extensions to include (no dot). |
| `--include-ignored`       | Include files ignored by `.gitignore`.               |
| `-h`, `--help`            | Print help information.                              |
| `-V`, `--version`         | Print version information.                           |

### Example

Scan the current directory, filter for Rust and Markdown files, and interactively select:

```bash
repoyank --types rs,md
```

After confirming your selections, `repoyank` will copy something like the following to your clipboard:

```
./
├─ src/
│  ├─ main.rs
│  └─ lib.rs
└─ README.md

---
File: src/main.rs
---
// (contents of main.rs)

---
File: README.md
---
# Project README
(…)
```

And tell you how much it copied:
```
✅ Copied 2 files (≈ 150 tokens) to the clipboard.
```

Paste anywhere to see a neat, combined view of your selections.

## Development

1. Clone the repo:

   ```bash
   ```
    git clone [https://github.com/TimKoornstra/repoyank.git](https://github.com/TimKoornstra/repoyank.git)
    cd repoyank
    ```

2. Build and run:
   ```bash
    cargo run -- --types rs,md
    ```

## Contributing

Contributions and feedback are welcome! Feel free to open issues or pull requests on [GitHub](https://github.com/TimKoornstra/repoyank).

## License

This project is licensed under the GPLv3 License. See [LICENSE](LICENSE) for details.

