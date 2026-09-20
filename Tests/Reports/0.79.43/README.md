# 0.79.43 targeted stability checks

2026-09-21; software/model evidence. Windows uses Build.bat with the same arguments.

```text
./Build.sh display-test '-Ddisplay-test-filter=producer exit preserves'
./Build.sh display-test '-Ddisplay-test-filter=queued dependencies reserve'
Two existing owner cases: capture/scanout/producer death at an exhausted budget;
queue timeout plus device loss and producer death before physical completion.
Every current BO counter returns to baseline; queues/resources are fully reaped.
No kernel artifact change. Both commands exited 0. Additional remote-frame
checks remain part of the existing display-test step.
```

Full software scope and physical exclusions: workspace Docs/Deployment/GrafikStabilitaet07943.txt.
