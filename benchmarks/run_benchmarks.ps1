#!/usr/bin/env pwsh
# Benchmark runner for the C compiler
# Compiles benchmarks with both our compiler and GCC, measures execution time

$benchmarks = @(
    "fib",
    "array_sum",
    "matmul",
    "bitwise",
    "struct_bench"
)

$ourCompiler = "target\debug\driver.exe"
$resultsFile = "benchmarks\results.md"

Write-Host "=== C Compiler Benchmark Suite ===" -ForegroundColor Cyan
Write-Host ""

# Build our compiler
Write-Host "Building compiler..." -ForegroundColor Yellow
cargo build --release 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Write-Host "Failed to build compiler!" -ForegroundColor Red
    exit 1
}
$ourCompiler = "target\release\driver.exe"

# Initialize results
$results = @"
# Benchmark Results

Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

| Benchmark | Our Compiler (ms) | GCC -O0 (ms) | GCC -O2 (ms) | Speedup vs GCC-O0 | Speedup vs GCC-O2 |
|-----------|-------------------|--------------|--------------|-------------------|-------------------|
"@

foreach ($bench in $benchmarks) {
    Write-Host "Running benchmark: $bench" -ForegroundColor Green
    
    $sourceFile = "benchmarks\$bench.c"
    
    # Compile with our compiler
    Write-Host "  Compiling with our compiler..." -ForegroundColor Gray
    & $ourCompiler $sourceFile 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  Failed to compile $bench with our compiler!" -ForegroundColor Red
        continue
    }
    $ourExe = "$bench.exe"
    
    # Compile with GCC -O0
    Write-Host "  Compiling with GCC -O0..." -ForegroundColor Gray
    gcc -O0 $sourceFile -o "benchmarks\${bench}_gcc_o0.exe" 2>&1 | Out-Null
    
    # Compile with GCC -O2
    Write-Host "  Compiling with GCC -O2..." -ForegroundColor Gray
    gcc -O2 $sourceFile -o "benchmarks\${bench}_gcc_o2.exe" 2>&1 | Out-Null
    
    # Run benchmarks (10 iterations each, take average)
    Write-Host "  Running benchmarks..." -ForegroundColor Gray
    
    $ourTimes = @()
    $gccO0Times = @()
    $gccO2Times = @()
    
    for ($i = 0; $i -lt 10; $i++) {
        # Our compiler
        $ourTime = Measure-Command { & ".\$ourExe" | Out-Null }
        $ourTimes += $ourTime.TotalMilliseconds
        
        # GCC -O0
        $gccO0Time = Measure-Command { & "benchmarks\${bench}_gcc_o0.exe" | Out-Null }
        $gccO0Times += $gccO0Time.TotalMilliseconds
        
        # GCC -O2
        $gccO2Time = Measure-Command { & "benchmarks\${bench}_gcc_o2.exe" | Out-Null }
        $gccO2Times += $gccO2Time.TotalMilliseconds
    }
    
    $ourAvg = ($ourTimes | Measure-Object -Average).Average
    $gccO0Avg = ($gccO0Times | Measure-Object -Average).Average
    $gccO2Avg = ($gccO2Times | Measure-Object -Average).Average
    
    $speedupO0 = [math]::Round($gccO0Avg / $ourAvg, 2)
    $speedupO2 = [math]::Round($gccO2Avg / $ourAvg, 2)
    
    Write-Host "    Our compiler: $([math]::Round($ourAvg, 2)) ms" -ForegroundColor Cyan
    Write-Host "    GCC -O0:      $([math]::Round($gccO0Avg, 2)) ms (${speedupO0}x)" -ForegroundColor Cyan
    Write-Host "    GCC -O2:      $([math]::Round($gccO2Avg, 2)) ms (${speedupO2}x)" -ForegroundColor Cyan
    
    $results += "`n| $bench | $([math]::Round($ourAvg, 2)) | $([math]::Round($gccO0Avg, 2)) | $([math]::Round($gccO2Avg, 2)) | ${speedupO0}x | ${speedupO2}x |"
    
    # Cleanup
    Remove-Item $ourExe -ErrorAction SilentlyContinue
    Remove-Item "benchmarks\${bench}_gcc_o0.exe" -ErrorAction SilentlyContinue
    Remove-Item "benchmarks\${bench}_gcc_o2.exe" -ErrorAction SilentlyContinue
}

$results += "`n`n## Notes`n"
$results += "- Times are averaged over 10 runs`n"
$results += "- Speedup > 1.0 means our compiler is faster`n"
$results += "- GCC -O0 is no optimization, GCC -O2 is standard optimizations`n"

$results | Out-File $resultsFile -Encoding UTF8
Write-Host ""
Write-Host "Results saved to $resultsFile" -ForegroundColor Green
