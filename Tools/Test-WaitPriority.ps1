param()
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$settings=@{}
foreach($line in [IO.File]::ReadAllLines((Join-Path $kernel 'Settings.R4S'))){
 $line=$line.Trim().TrimStart([char]0xFEFF)
 if($line -and !$line.StartsWith('#')){$key,$value=$line.Split('=',2);$settings[$key]=$value}
}
function Resolve-WorkPath([string]$Base,[string]$Relative){[IO.Path]::GetFullPath((Join-Path $Base $Relative.Replace('\',[IO.Path]::DirectorySeparatorChar)))}
$workspace=Resolve-WorkPath $kernel $settings.WORKSPACE_ROOT
$devkit=Resolve-WorkPath $workspace $settings.DEVKIT_ROOT
$starter=Join-Path $devkit 'Setup/Rust.ps1'
$work=Join-Path $workspace ('Temp/KernelWaitPriority/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 $index=Join-Path $kernel 'Rust/sched/task_index.rs'
 $queue=Join-Path $kernel 'Rust/sched/wait_node.rs'
 $cases=Join-Path $kernel 'Rust/sched/wait_node/host_tests.rs'
 $indexLiteral=$index.Replace('\','/')|ConvertTo-Json -Compress
 $queueLiteral=$queue.Replace('\','/')|ConvertTo-Json -Compress
 # Only the partial host crate suppresses unused complete-owner entry points.
 # Production uses these exact modules without a copied implementation.
 $crate="#![allow(dead_code)]`nmod sched {`n#[path = $indexLiteral] pub mod task_index;`n#[path = $queueLiteral] pub mod wait_node;`n}`n"
 $input=Join-Path $work 'wait_priority.rs';[IO.File]::WriteAllText($input,$crate)
 $executable=Join-Path $work $(if($IsWindows){'wait-priority.exe'}else{'wait-priority'})
 & $starter rustc --edition 2024 --test $input -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual wait priority/AVL owner modules did not compile.'}
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Actual wait priority/AVL owner checks failed.'}
 $sources=@{}
 foreach($path in @($index,$queue,$cases)){$sources[[IO.Path]::GetRelativePath($workspace,$path).Replace('\','/')]=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
 Write-Output (@{status='pass';owner_cases=12;source_sha256=$sources;temporary_wrapper='actual modules only; no copied implementation'}|ConvertTo-Json -Depth 4 -Compress)
}finally{
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}
}
