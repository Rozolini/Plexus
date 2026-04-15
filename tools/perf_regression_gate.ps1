param(
    [switch]$Quick
)

$ErrorActionPreference = "Stop"

$artifactDir = "artifacts\perf"
New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

$ipcOut = Join-Path $artifactDir "ipc_bench.txt"
$tcpOut = Join-Path $artifactDir "tcp_baseline.txt"

if ($Quick) {
    $env:PLEXUS_BENCH_ITERATIONS = "100000"
}

cargo bench --bench ipc_bench | Tee-Object -FilePath $ipcOut
cargo bench --bench grpc_tcp_baseline | Tee-Object -FilePath $tcpOut

$ipcText = Get-Content $ipcOut -Raw
$tcpText = Get-Content $tcpOut -Raw

# Bench binaries print summary line with fixed units used by this parser.
$ipcMatch = [regex]::Match($ipcText, "([0-9]+)\s+ns/msg")
$tcpMatch = [regex]::Match($tcpText, "([0-9]+)\s+ns/rt")

if (-not $ipcMatch.Success) {
    throw "Failed to parse IPC bench result from $ipcOut"
}
if (-not $tcpMatch.Success) {
    throw "Failed to parse TCP baseline result from $tcpOut"
}

$ipcNs = [double]$ipcMatch.Groups[1].Value
$tcpNs = [double]$tcpMatch.Groups[1].Value
$ratio = $ipcNs / $tcpNs

# Explicit regression thresholds for ops gating.
$maxIpcNs = 50000.0
$maxRatio = 1.20

$result = [PSCustomObject]@{
    ipc_ns_per_msg  = $ipcNs
    tcp_ns_per_rt   = $tcpNs
    ipc_to_tcp_ratio = [math]::Round($ratio, 4)
    max_ipc_ns_per_msg = $maxIpcNs
    max_ipc_to_tcp_ratio = $maxRatio
}

$json = $result | ConvertTo-Json -Depth 5
$jsonPath = Join-Path $artifactDir "perf_gate_result.json"
$json | Out-File -FilePath $jsonPath -Encoding utf8

Write-Host "Perf gate results:"
Write-Host $json

if ($ipcNs -gt $maxIpcNs) {
    throw "IPC ns/msg regression: measured $ipcNs, threshold $maxIpcNs"
}
if ($ratio -gt $maxRatio) {
    throw "IPC/TCP ratio regression: measured $ratio, threshold $maxRatio"
}

Write-Host "Perf regression gate passed."
