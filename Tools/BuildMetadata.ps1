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
    return $metadata
}
