# Simple PowerShell script
Write-Host "Hello, World!"

$numbers = 1, 2, 3, 4, 5
$sum = 0

foreach ($number in $numbers) {
    $sum += $number
}

Write-Host "Sum: $sum"