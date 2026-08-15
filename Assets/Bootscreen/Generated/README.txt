Build-Artefakte
===============

Dieser Ordner ist der Ausgabepfad fuer erzeugte Bootscreen-Artefakte.

`BOOTSCREEN.R4B` wird von `DevTools/Scripts/ConvertBootscreenBmp.zig` aus
`Code/Kernel/Assets/Bootscreen/BOOTSCREEN.BMP` erzeugt. Die `.R4B`-Datei ist ein
Build-Artefakt und wird nicht versioniert.

Ab 0.54.16 muss der normale Build dieses Artefakt erzeugen. Fehlt das BMP oder
ist es ungueltig, soll der Build fehlschlagen statt still auf einen alten
Bootscreen auszuweichen.

`Build.bat` kopiert diese Datei danach nach
`Code/Kernel/kernel/generated/BOOTSCREEN.R4B`, weil Zig eingebettete Dateien im
Kernel-Paketpfad erwartet. Diese Kopie ist ebenfalls ein Build-Artefakt und
nicht versioniert.
