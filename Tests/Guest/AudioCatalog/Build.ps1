param([string]$OutputRoot='')
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$workspace=[IO.Path]::GetFullPath((Join-Path $kernel '../..'))
if(!$OutputRoot){$OutputRoot=Join-Path $workspace 'Temp/AudioCatalogFixtures'}
$OutputRoot=[IO.Path]::GetFullPath($OutputRoot)
$sdk=Join-Path $workspace 'Repositories/SDK'
$zig=Join-Path $workspace ('DevKit/Toolchains/Zig/zig'+$(if($IsWindows){'.exe'}else{''}))
Push-Location $sdk
try {
 foreach($part in @('protocol','driver','observer','actual')) {
  $project=Join-Path $OutputRoot ('Projects/'+$part)
  [IO.Directory]::CreateDirectory((Join-Path $project 'src'))|Out-Null
  Copy-Item (Join-Path $PSScriptRoot ($part+'/module.R4MF')) (Join-Path $project 'module.R4MF')
  Copy-Item (Join-Path $PSScriptRoot ($part+'/src/main.zig')) (Join-Path $project 'src/main.zig')
  if($part -ne 'actual'){Copy-Item (Join-Path $PSScriptRoot 'shared.zig') (Join-Path $project 'src/shared.zig')}
  & $zig build --build-file (Join-Path $sdk 'Tools/ManifestProject/build.zig') ('--fork='+(Join-Path $workspace 'Repositories/Contract')) ('-Dproject-root='+$project) --cache-dir (Join-Path $OutputRoot 'Cache') --prefix (Join-Path $OutputRoot ('Artifacts/'+$part)) -j2 --summary all
  if($LASTEXITCODE -ne 0){throw ('Audio catalog fixture build failed: '+$part)}
 }
} finally {Pop-Location}
