param([int]$Count=129,[string]$OutputRoot='')
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
if($Count -lt 13){throw 'This explicit fixture must exercise the old12entry boundary.'}
$kernel=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$workspace=[IO.Path]::GetFullPath((Join-Path $kernel '../..'))
if(!$OutputRoot){$OutputRoot=Join-Path $workspace 'Temp/DriverCountFixtures'}
$OutputRoot=[IO.Path]::GetFullPath($OutputRoot)
$sdk=Join-Path $workspace 'Repositories/SDK';$contract=Join-Path $workspace 'Repositories/Contract'
$zig=Join-Path $workspace ('DevKit/Toolchains/Zig/zig'+$(if($IsWindows){'.exe'}else{''}))
[IO.Directory]::CreateDirectory($OutputRoot)|Out-Null
$artifacts=[Collections.Generic.List[object]]::new()
$sharedElf=$null
$packager=Join-Path $workspace ('Artifacts/SDK/HostTools/bin/'+$(if($IsWindows){'x86_64-pc-windows-msvc/r4xbuilder.exe'}else{'x86_64-unknown-linux-gnu/r4xbuilder'}))
function Find-CountFixtureElf([string]$name){
   $candidates=@(Get-ChildItem -LiteralPath (Join-Path $OutputRoot 'Cache/o') -Recurse -File | Where-Object{$_.Name -ceq ($name+'.elf')})
   $expected=(Get-FileHash (Join-Path $OutputRoot ('Artifacts/'+$name+'.R4D'))).Hash
   foreach($candidate in $candidates){
    $check=Join-Path $OutputRoot 'elf-provenance-check.R4D'
    & $packager --r4m --kind r4d --name ('R4D_'+$name) --output $check --elf $candidate.FullName --import R4DEV:Query:1 --export DriverInit:.text:0:1 --export DriverShutdown:.text:5:1 --meta ('r4d.name='+$name) --meta r4d.type=display --meta module.version=0.1.0
    if($LASTEXITCODE -ne 0){throw 'Canonical provenance repack failed'}
    if((Get-FileHash $check).Hash -ceq $expected){return $candidate.FullName;break}
   }
   throw 'No cached ELF reproduces the exact current canonical SDK-built fixture.'
}
for($i=-1;$i -lt $Count;$i++){
 $name=if($i -lt 0){'DC29BAD'}else{'DC29'+$i.ToString('D3')}
 $project=Join-Path $OutputRoot ('Projects/'+$name);[IO.Directory]::CreateDirectory($project)|Out-Null
 $text=[IO.File]::ReadAllText((Join-Path $PSScriptRoot 'driver.c'))
 if($i -lt 0){$text='#define COUNT_FAIL_INIT 1'+[Environment]::NewLine+$text}
 [IO.File]::WriteAllText((Join-Path $project 'main.c'),$text)
 $manifest=@('R4OS_MODULE_MANIFEST=2','KIND=R4D',('NAME='+$name),'VERSION=0.1.0','LANGUAGE=C','SOURCE=main.c',('TARGET=/R4OS/DRIVERS/'+$name+'.R4D'),'IMAGE_SCOPE=none','IMPORT=R4DEV:Query:1',('META=r4d.name='+$name),'META=r4d.type=display',('META=module.name=R4D_'+$name))
 [IO.File]::WriteAllLines((Join-Path $project 'module.R4MF'),$manifest)
 if($i -le 0){
 Push-Location $sdk
 try{& $zig build --build-file (Join-Path $sdk 'Tools/ManifestProject/build.zig') ('--fork='+$contract) ('-Dproject-root='+$project) --cache-dir (Join-Path $OutputRoot 'Cache') --prefix (Join-Path $OutputRoot 'Artifacts') -j1
  if($LASTEXITCODE -ne 0){throw ('Driver count fixture failed: '+$name)}
 }finally{Pop-Location}
  if($i -eq 0){
   $sharedElf=Find-CountFixtureElf $name
  }
 }else{
  # Container identities differ; executable source and its ordinary entry points
  # are identical. Reuse the actual SDK ELF and run the unchanged SDK packager.
  & $packager --r4m --kind r4d --name ('R4D_'+$name) --output (Join-Path $OutputRoot ('Artifacts/'+$name+'.R4D')) --elf $sharedElf --import R4DEV:Query:1 --export DriverInit:.text:0:1 --export DriverShutdown:.text:5:1 --meta ('r4d.name='+$name) --meta r4d.type=display --meta module.version=0.1.0
  if($LASTEXITCODE -ne 0){throw ('Canonical fixture packaging failed: '+$name)}
 }
 $artifact=Join-Path $OutputRoot ('Artifacts/'+$name+'.R4D')
 $artifacts.Add(@{name=$name;path=$artifact;sha256=(Get-FileHash $artifact).Hash.ToLowerInvariant();expected_active=($i -ge 0);shared_elf=$sharedElf})
 Write-Host ('Built explicit driver count fixture '+$name)
}
# A second intentional refusal occurs after all large owner/module tables grow.
$refusalElf=Find-CountFixtureElf 'DC29BAD'
$artifact=Join-Path $OutputRoot 'Artifacts/DC29END.R4D'
& $packager --r4m --kind r4d --name R4D_DC29END --output $artifact --elf $refusalElf --import R4DEV:Query:1 --export DriverInit:.text:0:1 --export DriverShutdown:.text:5:1 --meta r4d.name=DC29END --meta r4d.type=display --meta module.version=0.1.0
if($LASTEXITCODE -ne 0){throw 'Late refusal fixture packaging failed'}
$artifacts.Add(@{name='DC29END';path=$artifact;sha256=(Get-FileHash $artifact).Hash.ToLowerInvariant();expected_active=$false;shared_elf=$refusalElf})
[IO.File]::WriteAllText((Join-Path $OutputRoot 'fixtures.json'),($artifacts|ConvertTo-Json -Depth 4))
