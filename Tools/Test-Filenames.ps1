param()
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$evidence=Join-Path $workspace ('Temp/KernelFilenameTests/'+[guid]::NewGuid().ToString())
[void][IO.Directory]::CreateDirectory($evidence)
$executable=Join-Path $evidence $(if($IsWindows){'filename-tests.exe'}else{'filename-tests'})
$starter=Join-Path $workspace 'DevKit/Setup/Rust.ps1'
$target=Join-Path $evidence 'text-core'
& $starter cargo build --manifest-path (Join-Path $workspace 'Repositories/Libraries/R4STD/Rust/Cargo.toml') --target-dir $target
if($LASTEXITCODE -ne 0){throw 'Resident text core build failed.'}
$libraries=@(Get-ChildItem -LiteralPath (Join-Path $target 'debug/deps') -Filter 'libr4os_text_core-*.rlib')
if($libraries.Count -ne 1){throw 'Expected one exact text core artifact.'}
& $starter rustc --edition 2024 --test (Join-Path $kernel 'Rust/fs/name_host_tests.rs') --extern ('r4os_text_core='+$libraries[0].FullName) -D warnings -o $executable
if($LASTEXITCODE -ne 0){throw 'Native filesystem name test compilation failed.'}
& $executable --test-threads=1
if($LASTEXITCODE -ne 0){throw 'Native filesystem name tests failed.'}
@{status='pass';compile_exit=0;test_exit=0;test_count=3;software_only=$true;text_core_sha256=(Get-FileHash -LiteralPath $libraries[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $evidence 'Result.json') -Encoding utf8
Write-Host ('Filename test evidence: '+$evidence)
