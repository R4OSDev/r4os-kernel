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
$work=Join-Path $workspace ('Temp/KernelWorkContinuation/'+[guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($work)|Out-Null
try{
 $source=Join-Path $kernel 'Rust/driver/work_queue.rs'
 $executable=Join-Path $work $(if($IsWindows){'work.exe'}else{'work'})
 & $starter rustc --edition 2024 --test $source -O -D warnings -o $executable
 if($LASTEXITCODE -ne 0){throw 'Actual Work queue owner did not compile.'}
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Actual Work continuation owner checks failed.'}
 Write-Output (@{status='pass';owner_cases=5;retained_turns=50000;source_sha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json -Compress)
}finally{
 if(Test-Path -LiteralPath $work){Remove-Item -LiteralPath $work -Recurse -Force}
}
