# ============================================================================
# AnyTLS-RS Comprehensive Test Suite
# ============================================================================

param(
    [int]$ServerPort = 8443,
    [int]$ClientPort = 1080,
    [string]$Password = "test_password"
)

$ErrorActionPreference = "Continue"
$Global:TestResults = @()
$Global:TotalTests = 0
$Global:PassedTests = 0
$Global:FailedTests = 0

function Write-TestHeader {
    param([string]$Message)
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host $Message -ForegroundColor Cyan
    Write-Host "========================================`n" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-Failure {
    param([string]$Message)
    Write-Host "[FAIL] $Message" -ForegroundColor Red
}

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Blue
}

function Record-TestResult {
    param(
        [string]$TestName,
        [bool]$Passed,
        [string]$Details = "",
        [double]$Duration = 0
    )
    
    $Global:TotalTests++
    if ($Passed) {
        $Global:PassedTests++
        Write-Success "$TestName - Passed (${Duration}s)"
    } else {
        $Global:FailedTests++
        Write-Failure "$TestName - Failed"
        if ($Details) {
            Write-Host "  Details: $Details" -ForegroundColor Gray
        }
    }
    
    $Global:TestResults += [PSCustomObject]@{
        TestName = $TestName
        Passed = $Passed
        Details = $Details
        Duration = $Duration
    }
}

function Stop-TestProcesses {
    Write-Info "Stopping all test processes..."
    
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -match "anytls" } | ForEach-Object {
        Write-Info "  Stopping process: $($_.ProcessName) (PID: $($_.Id))"
        Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
    }
    
    Start-Sleep -Seconds 2
}

function Wait-ForPort {
    param(
        [int]$Port,
        [int]$TimeoutSeconds = 30
    )
    
    Write-Info "Waiting for port $Port to be ready..."
    $elapsed = 0
    
    while ($elapsed -lt $TimeoutSeconds) {
        $connection = New-Object System.Net.Sockets.TcpClient
        try {
            $connection.Connect("127.0.0.1", $Port)
            $connection.Close()
            Write-Success "Port $Port is ready"
            return $true
        } catch {
            Start-Sleep -Seconds 1
            $elapsed++
        } finally {
            if ($connection) {
                $connection.Dispose()
            }
        }
    }
    
    Write-Failure "Timeout waiting for port $Port"
    return $false
}

function Test-PreChecks {
    Write-TestHeader "Stage 0: Pre-checks"
    
    $cargoVersion = cargo --version 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Success "Cargo available: $cargoVersion"
    } else {
        Write-Failure "Cargo not found"
        exit 1
    }
    
    try {
        $curlVersion = & curl.exe --version 2>&1 | Select-Object -First 1
        Write-Success "Curl available: $curlVersion"
    } catch {
        Write-Failure "Curl not found"
        exit 1
    }
    
    Stop-TestProcesses
    Write-Success "Pre-checks completed"
}

function Test-Compilation {
    Write-TestHeader "Stage 1: Compilation Tests"
    
    Write-Info "Building release version..."
    $startTime = Get-Date
    $output = cargo build --release --bins 2>&1
    $duration = ((Get-Date) - $startTime).TotalSeconds
    
    if ($LASTEXITCODE -eq 0) {
        Record-TestResult "Release Build" $true "" $duration
    } else {
        Record-TestResult "Release Build" $false "Build failed" $duration
        Write-Host $output -ForegroundColor Red
        return $false
    }
    
    return $true
}

function Test-UnitTests {
    Write-TestHeader "Stage 2: Unit Tests"
    
    Write-Info "Running unit tests..."
    $startTime = Get-Date
    $output = cargo test --lib --  --test-threads=1 2>&1
    $duration = ((Get-Date) - $startTime).TotalSeconds
    
    if ($LASTEXITCODE -eq 0) {
        Record-TestResult "Unit Test Suite" $true "" $duration
    } else {
        Record-TestResult "Unit Test Suite" $false "Tests failed" $duration
        return $false
    }
    
    return $true
}

function Start-TestServices {
    Write-TestHeader "Stage 3: Starting Services"
    
    Write-Info "Starting AnyTLS Server (port $ServerPort)..."
    $serverArgs = @(
        "run", "--release", "--bin", "anytls-server", "--",
        "-l", "127.0.0.1:$ServerPort",
        "-p", $Password
    )
    
    $Global:ServerProcess = Start-Process -FilePath "cargo" `
        -ArgumentList $serverArgs `
        -NoNewWindow `
        -PassThru `
        -RedirectStandardOutput "server_output.log" `
        -RedirectStandardError "server_error.log"
    
    if (-not (Wait-ForPort -Port $ServerPort -TimeoutSeconds 30)) {
        Record-TestResult "Server Start" $false "Port not ready"
        return $false
    }
    
    Record-TestResult "Server Start" $true
    
    Write-Info "Starting AnyTLS Client (port $ClientPort)..."
    $clientArgs = @(
        "run", "--release", "--bin", "anytls-client", "--",
        "-l", "127.0.0.1:$ClientPort",
        "-s", "127.0.0.1:$ServerPort",
        "-p", $Password
    )
    
    $Global:ClientProcess = Start-Process -FilePath "cargo" `
        -ArgumentList $clientArgs `
        -NoNewWindow `
        -PassThru `
        -RedirectStandardOutput "client_output.log" `
        -RedirectStandardError "client_error.log"
    
    if (-not (Wait-ForPort -Port $ClientPort -TimeoutSeconds 30)) {
        Record-TestResult "Client Start" $false "Port not ready"
        return $false
    }
    
    Record-TestResult "Client Start" $true
    
    Write-Info "Waiting for services to stabilize..."
    Start-Sleep -Seconds 3
    
    return $true
}

function Test-BasicFunctionality {
    Write-TestHeader "Stage 4: Basic Functionality Tests"
    
    Write-Info "Test 4.1: Single HTTP GET request"
    $startTime = Get-Date
    $response = & curl.exe --socks5-hostname 127.0.0.1:$ClientPort --max-time 10 --silent --show-error http://httpbin.org/get 2>&1
    $duration = ((Get-Date) - $startTime).TotalSeconds
    
    if ($LASTEXITCODE -eq 0 -and $response -match '"url"') {
        Record-TestResult "Single GET Request" $true "" $duration
    } else {
        Record-TestResult "Single GET Request" $false "Invalid response" $duration
    }
    
    Start-Sleep -Seconds 1
}

function Test-ConsecutiveRequests {
    Write-TestHeader "Stage 5: Consecutive Requests Test (CORE TEST!)"
    
    Write-Info "This is the KEY test to verify the 'second request blocked' issue is fixed"
    
    $consecutiveResults = @()
    $totalDuration = 0
    
    for ($i = 1; $i -le 10; $i++) {
        Write-Info "Executing request $i..."
        $startTime = Get-Date
        
        $response = & curl.exe --socks5-hostname 127.0.0.1:$ClientPort --max-time 15 --silent --show-error http://httpbin.org/get 2>&1
        $duration = ((Get-Date) - $startTime).TotalSeconds
        $totalDuration += $duration
        
        if ($LASTEXITCODE -eq 0 -and $response -match '"url"') {
            $consecutiveResults += $true
            Write-Host "  [OK] Request $i succeeded (${duration}s)" -ForegroundColor Green
        } else {
            $consecutiveResults += $false
            Write-Host "  [FAIL] Request $i failed (${duration}s)" -ForegroundColor Red
            if ($i -eq 2) {
                Write-Host "  [WARN] Request 2 failed - this is the issue we're fixing!" -ForegroundColor Yellow
            }
        }
        
        Start-Sleep -Seconds 1
    }
    
    $successCount = ($consecutiveResults | Where-Object { $_ -eq $true }).Count
    $avgDuration = $totalDuration / 10
    
    if ($successCount -eq 10) {
        Record-TestResult "Consecutive 10 Requests" $true "All succeeded, avg ${avgDuration}s" $totalDuration
        Write-Success "CORE ISSUE FIXED! All consecutive requests succeeded!"
    } else {
        Record-TestResult "Consecutive 10 Requests" $false "Success $successCount/10" $totalDuration
        Write-Failure "Consecutive requests have failures (success rate: $successCount/10)"
        
        if ($consecutiveResults[1] -eq $false) {
            Write-Host "[WARN] Request 2 failed - core issue may NOT be fixed" -ForegroundColor Yellow
        }
    }
}

function Test-ConcurrentRequests {
    Write-TestHeader "Stage 6: Concurrent Requests Test"
    
    $concurrencyLevels = @(5, 10, 20)
    
    foreach ($concurrent in $concurrencyLevels) {
        Write-Info "Testing $concurrent concurrent requests..."
        
        $jobs = @()
        $startTime = Get-Date
        
        for ($i = 1; $i -le $concurrent; $i++) {
            $job = Start-Job -ScriptBlock {
                param($ClientPort)
                $response = & curl.exe --socks5-hostname 127.0.0.1:$ClientPort --max-time 30 --silent --show-error http://httpbin.org/get 2>&1
                
                if ($LASTEXITCODE -eq 0 -and $response -match '"url"') {
                    return $true
                } else {
                    return $false
                }
            } -ArgumentList $ClientPort
            
            $jobs += $job
        }
        
        Write-Info "Waiting for $concurrent tasks to complete..."
        $results = $jobs | Wait-Job | Receive-Job
        $jobs | Remove-Job
        
        $duration = ((Get-Date) - $startTime).TotalSeconds
        $successCount = ($results | Where-Object { $_ -eq $true }).Count
        
        if ($successCount -eq $concurrent) {
            Record-TestResult "Concurrent Test ($concurrent concurrent)" $true "All succeeded" $duration
        } else {
            Record-TestResult "Concurrent Test ($concurrent concurrent)" $false "Success $successCount/$concurrent" $duration
        }
        
        Start-Sleep -Seconds 2
    }
}

function Test-StressTest {
    Write-TestHeader "Stage 7: Stress Test"
    
    Write-Info "Executing 50 rapid consecutive requests..."
    
    $successCount = 0
    $failCount = 0
    $totalDuration = 0
    
    $startTime = Get-Date
    
    for ($i = 1; $i -le 50; $i++) {
        if ($i % 10 -eq 0) {
            Write-Info "  Progress: $i/50..."
        }
        
        $reqStart = Get-Date
        $response = & curl.exe --socks5-hostname 127.0.0.1:$ClientPort --max-time 15 --silent --show-error http://httpbin.org/get 2>&1
        $reqDuration = ((Get-Date) - $reqStart).TotalSeconds
        
        if ($LASTEXITCODE -eq 0 -and $response -match '"url"') {
            $successCount++
        } else {
            $failCount++
        }
        
        $totalDuration += $reqDuration
        Start-Sleep -Milliseconds 200
    }
    
    $duration = ((Get-Date) - $startTime).TotalSeconds
    $avgDuration = $totalDuration / 50
    $successRate = ($successCount / 50) * 100
    
    if ($successRate -ge 95) {
        Record-TestResult "Stress Test (50 requests)" $true "Success rate ${successRate}%, avg ${avgDuration}s" $duration
    } else {
        Record-TestResult "Stress Test (50 requests)" $false "Success rate only ${successRate}%" $duration
    }
}

function Generate-TestReport {
    Write-TestHeader "Test Report"
    
    Write-Host ""
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host "           Test Execution Summary" -ForegroundColor Cyan
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host ""
    
    Write-Host "Total Tests:   $Global:TotalTests" -ForegroundColor White
    Write-Host "Passed:        $Global:PassedTests" -ForegroundColor Green
    Write-Host "Failed:        $Global:FailedTests" -ForegroundColor Red
    
    $successRate = if ($Global:TotalTests -gt 0) { 
        ($Global:PassedTests / $Global:TotalTests) * 100 
    } else { 
        0 
    }
    
    $successRateColor = if ($successRate -ge 95) { "Green" } elseif ($successRate -ge 80) { "Yellow" } else { "Red" }
    Write-Host "Success Rate:  $([math]::Round($successRate, 2))%" -ForegroundColor $successRateColor
    
    Write-Host ""
    
    if ($Global:FailedTests -gt 0) {
        Write-Host "Failed Tests:" -ForegroundColor Red
        $Global:TestResults | Where-Object { -not $_.Passed } | ForEach-Object {
            Write-Host "  - $($_.TestName)" -ForegroundColor Red
            if ($_.Details) {
                Write-Host "    $($_.Details)" -ForegroundColor Gray
            }
        }
        Write-Host ""
    }
    
    Write-Host "Slowest 5 Tests:" -ForegroundColor Yellow
    $Global:TestResults | Sort-Object Duration -Descending | Select-Object -First 5 | ForEach-Object {
        $durationStr = [math]::Round($_.Duration, 2)
        Write-Host "  - $($_.TestName): ${durationStr}s" -ForegroundColor Gray
    }
    
    Write-Host ""
    Write-Host "============================================" -ForegroundColor Cyan
    if ($successRate -ge 95) {
        Write-Host "Test Rating: EXCELLENT" -ForegroundColor Green
        Write-Host "Refactor successful! All key tests passed!" -ForegroundColor Green
    } elseif ($successRate -ge 80) {
        Write-Host "Test Rating: GOOD" -ForegroundColor Yellow
        Write-Host "Most tests passed, but room for improvement" -ForegroundColor Yellow
    } else {
        Write-Host "Test Rating: NEEDS IMPROVEMENT" -ForegroundColor Red
        Write-Host "Multiple test failures, further investigation needed" -ForegroundColor Red
    }
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host ""
    
    $reportPath = "test_report_$(Get-Date -Format 'yyyyMMdd_HHmmss').txt"
    $Global:TestResults | Format-Table -AutoSize | Out-File -FilePath $reportPath
    Write-Info "Detailed report saved to: $reportPath"
    
    Write-Host ""
    Write-Info "Service log files:"
    if (Test-Path "server_output.log") {
        $serverLines = (Get-Content "server_output.log" | Measure-Object -Line).Lines
        Write-Host "  - server_output.log ($serverLines lines)" -ForegroundColor Gray
    }
    if (Test-Path "client_output.log") {
        $clientLines = (Get-Content "client_output.log" | Measure-Object -Line).Lines
        Write-Host "  - client_output.log ($clientLines lines)" -ForegroundColor Gray
    }
}

function Main {
    $scriptStart = Get-Date
    
    Write-Host ""
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host "  AnyTLS-RS Comprehensive Test Suite" -ForegroundColor Cyan
    Write-Host "============================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Info "Test start time: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
    Write-Info "Server port: $ServerPort"
    Write-Info "Client port: $ClientPort"
    Write-Host ""
    
    try {
        Test-PreChecks
        
        if (-not (Test-Compilation)) {
            Write-Failure "Compilation failed, tests aborted"
            return
        }
        
        if (-not (Test-UnitTests)) {
            Write-Host "[WARN] Unit tests failed, but continuing with integration tests" -ForegroundColor Yellow
        }
        
        if (-not (Start-TestServices)) {
            Write-Failure "Service startup failed, tests aborted"
            return
        }
        
        Test-BasicFunctionality
        Test-ConsecutiveRequests
        Test-ConcurrentRequests
        Test-StressTest
        
    } finally {
        Write-Host ""
        Write-Info "Cleaning up test environment..."
        Stop-TestProcesses
        
        $scriptDuration = ((Get-Date) - $scriptStart).TotalSeconds
        Write-Info "Total test duration: $([math]::Round($scriptDuration, 2)) seconds"
        
        Generate-TestReport
    }
}

Main
