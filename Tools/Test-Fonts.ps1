param([string]$FontArtifacts='')
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$workspace=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(!$FontArtifacts){$FontArtifacts=Join-Path $workspace 'Artifacts/Distribution/HostTools/share/r4os/fonts'}
$FontArtifacts=[IO.Path]::GetFullPath($FontArtifacts,$workspace)
if(!(Test-Path -LiteralPath $FontArtifacts -PathType Container)){throw 'Build Distribution system fonts first.'}
$evidence=Join-Path $workspace ('Temp/KernelFontTests/'+[guid]::NewGuid().ToString())
[void][IO.Directory]::CreateDirectory($evidence)
$executable=Join-Path $evidence $(if($IsWindows){'font-tests.exe'}else{'font-tests'})
$starter=Join-Path $workspace 'DevKit/Setup/Rust.ps1'
& $starter rustc --edition 2024 --test (Join-Path $kernel 'Rust/font/host_tests.rs') -D warnings -o $executable
if($LASTEXITCODE -ne 0){throw 'Native font test compilation failed.'}
$previous=[Environment]::GetEnvironmentVariable('R4OS_FONT_ARTIFACTS','Process')
try{
 $env:R4OS_FONT_ARTIFACTS=$FontArtifacts
 & $executable --test-threads=1
 if($LASTEXITCODE -ne 0){throw 'Native font decoder tests failed.'}
 @{status='pass';compile_exit=0;test_exit=0;test_count=4;software_only=$true;fonts=$FontArtifacts}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $evidence 'Result.json') -Encoding utf8
}finally{[Environment]::SetEnvironmentVariable('R4OS_FONT_ARTIFACTS',$previous,'Process')}
Write-Host ('Font test evidence: '+$evidence)
