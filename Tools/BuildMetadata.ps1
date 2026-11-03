function Get-R4KernelBuildMetadata {
    param([Parameter(Mandatory)][string]$KernelRoot,[string]$Artifact='')
    if (-not $Artifact) { $Artifact=Join-Path $KernelRoot 'zig-out/bin/r4os.elf' }
    $path=Join-Path $KernelRoot 'zig-out/bin/r4os.build.json'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw 'Kernel build metadata is missing; rebuild with Kernel/Build.sh or Build.bat.' }
    $metadata=Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable
    $hash=(Get-FileHash -LiteralPath $Artifact -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($metadata.schema -ne 1 -or $metadata.artifact -cne 'r4os.elf' -or $metadata.sha256 -cne $hash) {
        throw 'Kernel build metadata does not describe this exact ELF; rebuild before exporting compiler/profile data.'
    }
    if ($metadata.languages.Count -eq 0 -or @($metadata.languages | Where-Object {$_ -cnotin @('zig','rust','assembly')}).Count) { throw 'Invalid Kernel implementation languages.' }
    if ($metadata.languages -ccontains 'rust') {
        $profile=$metadata.profiles.rust
        if ($profile.target -cne 'x86_64-unknown-none' -or $profile.overflow_checks -ne $true -or
            $profile.debug_assertions -ne $true -or $profile.panic -cne 'abort' -or $profile.red_zone -ne $false -or
            $profile.simd -ne $false -or $profile.code_model -cne 'kernel' -or $profile.lto -ne $false) {
            throw 'Kernel Rust profile differs from its migration contract.'
        }
    }
    if($metadata.ContainsKey('sdk_profile')){
        $settings=@{}
        foreach($line in [IO.File]::ReadAllLines((Join-Path $KernelRoot 'Settings.R4S'))){if($line -match '^([A-Z_]+)=(.+)$'){$settings[$Matches[1]]=$Matches[2]}}
        $repositories=[IO.Path]::GetFullPath($settings.REPOSITORIES_ROOT.Replace('\','/'),$KernelRoot)
        $sdk=[IO.Path]::GetFullPath($settings.SDK_ROOT.Replace('\','/'),$repositories)
        . (Join-Path $sdk 'Tools/ProfileData.ps1')
        $data=Read-R4SdkProfiles $sdk
        if($metadata.sdk_profile.id -cne 'r4os-kernel-rust-v1'){throw 'KernelSdkProfileIdentityMismatch'}
        foreach($name in $data.hashes.Keys){if($metadata.sdk_profile.hashes[$name] -cne $data.hashes[$name]){throw 'KernelSdkProfileSourceMismatch; rebuild Kernel with selected SDK.'}}
        $codegen=$data.Build.codegen.kernel
        if(($metadata.profiles.rust.rustflags -join [char]31) -cne ($codegen.rustflags -join [char]31) -or
            ($metadata.profiles.rust.linker_flags -join [char]31) -cne ($codegen.linker_flags -join [char]31)) {throw 'KernelSdkFlagsMismatch'}
    }
    return $metadata
}
