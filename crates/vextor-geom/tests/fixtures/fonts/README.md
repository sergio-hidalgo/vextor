# Test fonts

Noto Sans Regular and Bold, © 2022 The Noto Project Authors, licensed under
the SIL Open Font License 1.1 (`OFL.txt`). Source:
`notofonts.github.io/fonts/NotoSans/hinted/ttf/` (2026-10-19).

Subset with fontTools `pyftsubset` to Basic Latin, Latin-1 and common
punctuation (`U+0020-007E, U+00A0-00FF, – — ‘ ’ “ ” … €`), hinting dropped,
layout features `kern liga calt ccmp locl mark mkmk` kept. 1 000 units per
em. Used only by tests: layout tests never depend on the machine's fonts.
