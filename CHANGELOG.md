# Changelog

## [Unreleased]

### Added

- Allow switching between different primitive type representations
- Add a "Save" button to easily save reconstructed types into files
- Add keyboard shortcuts for opening PDB files and saving reconstructed types (Ctrl+O and Ctrl+S respectively)
- Allow changing the editor's font size via the settings menu
- Add a `dump-all` command to `resymc`, which dumps all types in a given PDB file (proposal by @xarkes)

### Fixed

- Reconstruct access specifiers for base classes (@TrinityDevelopers)
- Reconstruct type qualifiers for member functions (@TrinityDevelopers)
- Fix reconstruction of function pointer return types for member functions (@TrinityDevelopers)
- Fix incorrect reconstruction of class/struct and union destructors (@TrinityDevelopers)
- Fix "File" menu not closing when clicking a button (@mrexodia)
- Fix field offsets and struct/classes/unions sizes being truncated when greater than 2^16 (@xarkes)
- Fix the `list` command not outputting new lines in output files in `resymc` (@xarkes)
- Fix incorrect reconstruction of bitfields as unions
- Reconstruct C++20's **char8_t** primitive type

## [0.2.0] - 2022-05-22

### Added

- Command-line version of the tool (`resymc`)
- Syntax highlighting (both in the GUI and in the CLI version of the tool)
- Basic type diffing capability
- Line numbering (only for the GUI version of the tool)

## [0.1.0] - 2022-05-04

Initial release
