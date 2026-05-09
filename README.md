# repoyank

`repoyank` is a CLI tool specifically designed to help you interactively select and format code snippets from your repository, perfect for easily preparing structured input for large language models (LLMs) without the need to upload your sensitive data to third-party services.

## 🚀 Key Benefits

- **Privacy-first:** Keep your codebase secure by preparing snippets locally instead of uploading to external tools.
- **LLM-friendly output:** Neatly formatted for direct copy-paste into GPT or other language models.
- **Interactive Tree View:** Easily navigate your repository and precisely select files and directories.
- **Powerful Pattern Matching:** Use shell-style globs to define the scope of your selection.

## 🎯 Features

- **Flexible File Scoping:** Specify target files and directories using intuitive glob patterns.
- **Interactive Selection:** Refine your selection with a tree-view interface.
- **Recursive File Inclusion:** Automatically gathers nested files within selected directories.
- **Structured Clipboard Output:** Provides well-formatted snippets with clear file separation.
- **Customizable File Filtering:** Easily include specific file types by extension.
- **TUI Pre-selection:** Highlight items matching glob patterns on TUI start-up.
- **Direct Yanking (`--all`):** Skip the TUI and directly yank files matching patterns and filters.
- **Dry Run Mode:** Preview what would be selected and copied without touching the clipboard.
- **File Output:** Use `-o <file>` to write the generated output directly to a file instead of copying it.
- **Clipboard Integration:** Works smoothly across Linux (Wayland/X11), macOS, and Windows via `arboard`.
- **Git-aware:** Optional inclusion of files normally ignored by `.gitignore`.

## 📥 Installation

There are three easy ways to install `repoyank`:

1. **Via crates.io (Rust ecosystem)**

   ```bash
   cargo install repoyank
   ```

2. **Via AUR (Arch Linux)**
   If you use an AUR helper like `paru` or `yay`, simply run:

   ```bash
   paru -S repoyank
   # or
   yay -S repoyank
   ```

3. **Latest development version (from GitHub)**

   ```bash
   cargo install --git https://github.com/TimKoornstra/repoyank.git --branch main
   ```

## 🛠 Usage

```bash
repoyank [OPTIONS] [PATTERN ...]
```

### Arguments

*   `[PATTERN ...]`
    *   Zero or more shell-style globs (e.g., `src/**/*.rs`, `docs/*.md`, `path/to/specific_file.txt`).
    *   Globs are resolved relative to the **scan root**.
    *   **Scan Root:**
        *   If the first `PATTERN` provided is an existing directory, it is used as the scan root.
        *   Otherwise, the current working directory (`.`) is the scan root.
    *   If no patterns are given, `repoyank` defaults to scanning all files (`**/*`) under the scan root.

### Options

| Short | Long / value            | Purpose & Notes                                                                                                     |
| :---- | :---------------------- | :------------------------------------------------------------------------------------------------------------------ |
| `-a`  | `--all`                 | Skip the TUI entirely – yank everything selected by patterns and filters.                                           |
| `-t`  | `--type <EXT[,EXT...]>` | Filter by comma-separated file extensions (e.g., `rs,md`; no dots). Applied *after* patterns.                        |
| `-s`  | `--select <GLOB[,...]>` | Pre-select items in the TUI matching these globs. Globs are relative to the scan root. User can still change pick. |
| `-i`  | `--include-ignored`     | Include files that are normally excluded by `.gitignore`.                                                             |
| `-n`  | `--dry-run`             | Print the final tree and selection summary, but **don't** touch the clipboard.                                    |
| `-o`  | `--output <FILE>`       | Write generated output to `FILE` instead of copying to the clipboard.                                              |
| `-h`  | `--help`                | Show help information.                                                                                              |
| `-V`  | `--version`             | Show version information.                                                                                           |

*(Deprecated aliases like `--headless` and `--preselect` may still work for a limited time but will be removed in a future version.)*

### Examples

1.  **Browse the current directory and cherry-pick files:**
    ```bash
    repoyank
    ```

2.  **Browse a specific subdirectory (`my_project/src`) and pick files:**
    ```bash
    repoyank my_project/src
    ```
    *(Here, `my_project/src` becomes the scan root, and the default pattern `**/*` is applied within it.)*

3.  **Interactively select only Python files from the current directory:**
    ```bash
    repoyank -t py '**/*.py'
    # or more simply, if you want all python files as candidates:
    repoyank -t py
    ```

4.  **Pre-highlight all C++ test files in the TUI for review:**
    ```bash
    repoyank -s 'tests/**/*.cpp' src/ include/ tests/
    ```

5.  **Instantly yank (skip TUI) all C++ test files:**
    ```bash
    repoyank -a 'tests/**/*.cpp'
    ```

6.  **Instantly yank all Rust and Markdown files from the `src` and `docs` directories:**
    ```bash
    repoyank -a -t rs,md src/ docs/
    ```

7.  **See what would be yanked from Markdown files in `docs/`, without copying:**
    ```bash
    repoyank -n -a 'docs/**/*.md'
    ```

8.  **Write all matching Rust files to a file, including from a subdirectory:**
    ```bash
    repoyank -a -t rs src/ -o /tmp/repoyank-output.txt
    ```

9.  **Use a relative output path from your current directory:**
    ```bash
    repoyank -a 'docs/**/*.md' -o exports/docs-snippet.md
    ```

8.  **Include generated files (e.g., in `build/`) that are in `.gitignore`:**
    ```bash
    repoyank -i 'build/**/*'
    ```

### Output Format

After selection (or direct yanking), your clipboard will contain output like:

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

And `repoyank` will provide a helpful confirmation on your console, including the tree structure that was copied or saved:

```
./
├─ src/
│  └─ main.rs
└─ README.md

✅ Copied 2 files (≈ 150 tokens) from the displayed tree to the clipboard.
```

## 💻 Development

Clone and run locally:

```bash
git clone https://github.com/TimKoornstra/repoyank.git
cd repoyank
cargo run -- -t rs,md src/
```

## 🤝 Contributing

This is my first Rust project! Contributions, suggestions, and improvements are very welcome. Feel free to open issues or pull requests at [GitHub](https://github.com/TimKoornstra/repoyank).

## 📄 License

This project is licensed under the GPLv3 License. See [LICENSE](LICENSE) for details.

