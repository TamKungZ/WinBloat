<p align="center">
  <img src="assets/icon.ico" width="48" alt="WinBloat icon">
</p>

# WinBloat

WinBloat focuses on the CLI. The GUI is an experimental extra and is not actively developed.

Read-only disk usage scanner for Windows. Browse a directory tree, find large files and folders, and see size totals by file type.

```text
winbloat [PATH]                # CLI report
winbloat --mode tui [PATH]     # Terminal interface
winbloat --mode gui [PATH]     # Window with tree, details, and treemap
```

<p align="center">
  <img src="assets/screenshots/3A0A0442-7D2F-4DAE-9AE2-669A817DE405.png" height="480">
  <img src="assets/screenshots/5DE31721-F623-45A3-8EBC-4125132CBA90.png" height="480">
</p>

Sizes are logical file sizes; allocated disk space and filesystem attributes are not collected.

Licensed under the [MIT License](LICENSE).
