R4OS Kernel
===========

Dieses Repository enthaelt den R4OS-Kernel fuer x86_64, seine Limine-
Bootintegration, den Linker, zwingend eingebaute Bestandteile und kernelnahe
Tests. Der Kernel wird fest in ReleaseSafe und ohne SIMD gebaut.

Build
-----

Unter Windows:

    Build.bat
    Build.bat test

Unter Linux und macOS:

    ./Build.sh
    ./Build.sh test

`Settings.R4S` ordnet Workspace, Repositories, Contract, DevKit und Zig zu.
Alle Werte duerfen relativ oder absolut gesetzt werden. Die Buildstarter sind
der verbindliche Einstieg, weil sie den gemappten Contract-Checkout vor der
Zig-Paketaufloesung als lokalen Fork einsetzen.

Grenze
------

Der generierte Plattformvertrag kommt ausschliesslich aus dem gepinnten
`r4os-contract`. `program/r4x_api.zig` bleibt die handgeschriebene, stabile
Kernel-Fassade. Der Kernel importiert keine SDK-, Library-, Modul- oder
Distributionsquelle und kennt keine optionale Runtime-Library beim Namen.

Die fuenf Dateien unter `Support/` sind bytegenaue, kernel-eigene Snapshots
der beim Referenzbuild eingebundenen, allokationsfreien Kernhelfer. Ihre
Herkunft und die Migrationsgrenze stehen in `PROVENANCE.txt`.
