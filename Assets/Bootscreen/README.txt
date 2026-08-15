Bootscreen-Asset
================

Dieser Ordner ist der geplante Quellort fuer das bearbeitbare
Bootscreen-Hintergrundbild.

Geplanter finaler Pfad:

`Code/Kernel/Assets/Bootscreen/BOOTSCREEN.BMP`

Vertrag:
- Aufloesung: exakt `1280x720`
- Format: unkomprimiertes 24-bpp- oder 32-bpp-BMP
- keine Palette
- keine BMP-Kompression

Das BMP ist Build-Input, keine Bootzeit-Datei. Der Build konvertiert es in ein
einfaches R4B-Artefakt unter `Code/Kernel/Assets/Bootscreen/Generated/BOOTSCREEN.R4B`,
das der Kernel einbettet. Der Fortschrittsbalken wird weiterhin dynamisch vom
Kernel ueber das Bild gezeichnet.

Der Kernel zeichnet das 1280x720-Bild auf einem exakt gleich grossen
Framebuffer unveraendert ab Ursprung 0,0. Auf groesseren Framebuffern wird es
pixelgenau und ohne Skalierung zentriert; bei 1920x1080 beginnt es deshalb bei
320,180. Ist die Breite kleiner als 1280 oder die Hoehe kleiner als 720, wird
das Bild weiterhin nicht angezeigt.

Ab 0.54.16 ist `BOOTSCREEN.BMP` verpflichtender Build-Input. Wenn die Datei
fehlt oder vom Vertrag abweicht, bricht der Build beim Konvertierungsschritt ab.
Der Kernel hat keinen BMP-Decoder und liest zur Bootzeit keine Bilddatei.

`Generated/BOOTSCREEN.R4B` ist ein Build-Artefakt und wird nicht versioniert.
Beim normalen Build wird es aus dem BMP neu erzeugt und danach in den Kernel
eingebettet.
