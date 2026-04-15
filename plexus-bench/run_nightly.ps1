param(
    [int64]$Iterations = 1000000
)

$ErrorActionPreference = "Stop"
# Keep decimal separators stable for parsing/markdown across locale settings.
[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture
[System.Threading.Thread]::CurrentThread.CurrentUICulture = [System.Globalization.CultureInfo]::InvariantCulture

$artifactDir = "artifacts"
if (-not (Test-Path $artifactDir)) {
    New-Item -ItemType Directory -Path $artifactDir | Out-Null
}

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
$artifactFile = Join-Path $artifactDir "nightly-$timestamp.md"

function Parse-Metric {
    param(
        [string]$Text,
        [string]$Pattern,
        [string]$MetricName
    )
    # Parse from stdout so one script can be used locally and in CI without extra JSON output mode.
    $match = [regex]::Match($Text, $Pattern)
    if (-not $match.Success) {
        throw "Failed to parse $MetricName from benchmark output."
    }
    return $match.Groups[1].Value
}

Write-Host "Running TCP baseline..."
$tcpOutput = cargo run --release -- --mode tcp --iterations $Iterations | Out-String
Write-Host "Running Plexus benchmark..."
$plexusOutput = cargo run --release -- --mode plexus --iterations $Iterations | Out-String

$tcpTime = [double](Parse-Metric -Text $tcpOutput -Pattern "Total Time:\s+([0-9]+\.[0-9]+)\s+s" -MetricName "TCP total time")
$tcpLatency = [double](Parse-Metric -Text $tcpOutput -Pattern "Avg Latency:\s+([0-9]+)\s+ns/msg" -MetricName "TCP avg latency")
$tcpThroughput = [double](Parse-Metric -Text $tcpOutput -Pattern "Throughput:\s+([0-9]+\.[0-9]+)\s+msg/s" -MetricName "TCP throughput")

$plexusTime = [double](Parse-Metric -Text $plexusOutput -Pattern "Total Time:\s+([0-9]+\.[0-9]+)\s+s" -MetricName "Plexus total time")
$plexusLatency = [double](Parse-Metric -Text $plexusOutput -Pattern "Avg Latency:\s+([0-9]+)\s+ns/msg" -MetricName "Plexus avg latency")
$plexusThroughput = [double](Parse-Metric -Text $plexusOutput -Pattern "Throughput:\s+([0-9]+\.[0-9]+)\s+msg/s" -MetricName "Plexus throughput")

$speedup = $tcpTime / $plexusTime
$latencyGain = $tcpLatency / $plexusLatency
$throughputGain = $plexusThroughput / $tcpThroughput

$utcDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd")

$reportLines = @(
    "# Nightly Benchmark Report ($utcDate)",
    "",
    "## Configuration",
    "",
    "- Iterations: $Iterations",
    "- Build: release",
    "",
    "## Results",
    "",
    "| Metric | TCP + JSON Baseline | Plexus Zero-Copy | Difference |",
    "| --- | ---: | ---: | ---: |",
    "| Total Time | $("{0:F6}" -f $tcpTime) s | $("{0:F6}" -f $plexusTime) s | $("{0:F2}" -f $speedup)x speedup |",
    "| Avg Latency | $("{0:F0}" -f $tcpLatency) ns/msg | $("{0:F0}" -f $plexusLatency) ns/msg | $("{0:F2}" -f $latencyGain)x lower latency |",
    "| Throughput | $("{0:F2}" -f $tcpThroughput) msg/s | $("{0:F2}" -f $plexusThroughput) msg/s | $("{0:F2}" -f $throughputGain)x higher throughput |",
    "",
    "## Raw Output (TCP)",
    "",
    # Preserve raw benchmark output in the artifact for post-run validation/debugging.
    $tcpOutput.TrimEnd(),
    "",
    "## Raw Output (Plexus)",
    "",
    $plexusOutput.TrimEnd()
)
$report = $reportLines -join [Environment]::NewLine

$report | Out-File -FilePath $artifactFile -Encoding utf8

Write-Host ""
Write-Host "=== Nightly KPI Summary ==="
Write-Host "Date (UTC): $utcDate"
Write-Host "Iterations: $Iterations"
Write-Host "TCP Time: $tcpTime s"
Write-Host "Plexus Time: $plexusTime s"
Write-Host "Speedup: $([math]::Round($speedup, 2))x"
Write-Host "TCP Latency: $tcpLatency ns/msg"
Write-Host "Plexus Latency: $plexusLatency ns/msg"
Write-Host "Throughput Gain: $([math]::Round($throughputGain, 2))x"
Write-Host "Artifact: $artifactFile"
