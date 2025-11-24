# ForgeCode Deployment Script
param(
    [Parameter(Mandatory=$true)]
    [string]$Environment,
    
    [Parameter(Mandatory=$false)]
    [string]$Version = "latest",
    
    [switch]$DryRun,
    [switch]$Force
)

# Import required modules
Import-Module WebAdministration -ErrorAction Stop

# Configuration
$Config = @{
    Production = @{
        AppPool = "ForgeCodeProd"
        Site = "ForgeCode"
        Path = "C:\inetpub\ForgeCode"
        Url = "https://forgecode.dev"
    }
    Staging = @{
        AppPool = "ForgeCodeStaging"
        Site = "ForgeCode-Staging"
        Path = "C:\inetpub\ForgeCode-Staging"
        Url = "https://staging.forgecode.dev"
    }
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "[$timestamp] [$Level] $Message"
}

function Test-Prerequisites {
    Write-Log "Checking prerequisites..."
    
    if (-not (Test-Path -Path $Config[$Environment].Path)) {
        throw "Deployment path does not exist: $($Config[$Environment].Path)"
    }
    
    if (-not (Get-WebAppPoolState -Name $Config[$Environment].AppPool)) {
        throw "Application pool not found: $($Config[$Environment].AppPool"
    
    Write-Log "Prerequisites check passed"
}

function Start-Deployment {
    Write-Log "Starting deployment to $Environment environment..."
    
    try {
        Test-Prerequisites
        
        if (-not $DryRun) {
            # Stop application pool
            Write-Log "Stopping application pool..."
            Stop-WebAppPool -Name $Config[$Environment].AppPool
            
            # Deploy files
            Write-Log "Deploying files..."
            $sourcePath = ".\dist\$Version"
            Copy-Item -Path $sourcePath\* -Destination $Config[$Environment].Path -Recurse -Force
            
            # Start application pool
            Write-Log "Starting application pool..."
            Start-WebAppPool -Name $Config[$Environment].AppPool
            
            Write-Log "Deployment completed successfully"
        } else {
            Write-Log "Dry run mode - no changes made"
        }
    }
    catch {
        Write-Log "Deployment failed: $($_.Exception.Message)" -Level "ERROR"
        throw
    }
}

# Execute deployment
try {
    Start-Deployment
    Write-Log "Deployment process completed"
catch {
    Write-Log "Deployment process failed" -Level "ERROR"
    exit 1
}

# Missing closing brace for try-catch