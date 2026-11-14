function Invoke-CheckedNative {
    param([string]$FilePath, [string[]]$Arguments)
    # A pipeline makes PowerShell wait for Windows GUI-subsystem executables too.
    & $FilePath @Arguments | ForEach-Object { $_ }
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}
