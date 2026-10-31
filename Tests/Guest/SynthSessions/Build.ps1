param([string]$OutputRoot='', [ValidateSet('protocol','driver','child','controller','deep','fault','fault-controller')][string[]]$Part=@('protocol','driver','child','controller','deep','fault','fault-controller'), [string]$CacheRoot='')
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$workspace=[IO.Path]::GetFullPath((Join-Path $kernel '../..'))
if(!$OutputRoot){$OutputRoot=Join-Path $workspace 'Temp/SynthSessionsFixtures'}
$OutputRoot=[IO.Path]::GetFullPath($OutputRoot)
if(!$CacheRoot){$CacheRoot=Join-Path $OutputRoot 'Cache'}
$CacheRoot=[IO.Path]::GetFullPath($CacheRoot)
$sdk=Join-Path $workspace 'Repositories/SDK'
$zig=Join-Path $workspace ('DevKit/Toolchains/Zig/zig'+$(if($IsWindows){'.exe'}else{''}))
Push-Location $sdk
try {
 foreach($part in $Part) {
  $project=Join-Path $OutputRoot ('Projects/'+$part)
  [IO.Directory]::CreateDirectory((Join-Path $project 'src'))|Out-Null
  Copy-Item (Join-Path $PSScriptRoot ($part+'/module.R4MF')) (Join-Path $project 'module.R4MF')
  Copy-Item (Join-Path $PSScriptRoot ($part+'/src/main.zig')) (Join-Path $project 'src/main.zig')
  Copy-Item (Join-Path $PSScriptRoot 'shared.zig') (Join-Path $project 'src/shared.zig')
  & $zig build --build-file (Join-Path $sdk 'Tools/ManifestProject/build.zig') ('--fork='+(Join-Path $workspace 'Repositories/Contract')) ('-Dproject-root='+$project) --cache-dir $CacheRoot --prefix (Join-Path $OutputRoot ('Artifacts/'+$part)) -j2 --summary all
  if($LASTEXITCODE -ne 0){throw ('Synth session fixture build failed: '+$part)}
 }
} finally {Pop-Location}
