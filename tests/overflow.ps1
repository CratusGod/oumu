# Shared by the headless suites: text that sticks out past its container on the right
# (it is cut off or runs over its neighbour). Reads a Rune --tree file. Keep this ASCII.
function Get-Overflows([string]$path) {
    if (-not (Test-Path $path)) { return @() }
    $j = Get-Content -Raw -Encoding UTF8 $path | ConvertFrom-Json
    $script:ovf = New-Object System.Collections.ArrayList
    function Walk($n, [double]$right) {
        $b = $n.bounds
        if ($b) {
            $r = [double]$b[0] + [double]$b[2]
            $isText = $n.role -eq "label" -or $n.role -eq "text" -or $n.role -eq "statictext"
            # count badges (7, 99+) sit over a corner on purpose
            $badge = $n.name -match "^[0-9]+[+]?$"
            if ($isText -and $n.name -and -not $badge -and $b[2] -gt 0 -and $r -gt $right + 1.5) {
                [void]$script:ovf.Add(("{0} (+{1:N0}px)" -f $n.name.Substring(0, [Math]::Min(40, $n.name.Length)), ($r - $right)))
            }
            # a scroll area's content may be wider than it is (that's what scrolling is for)
            $scrolls = $n.actions -and ($n.actions -contains "scroll-left" -or $n.actions -contains "scroll-right")
            if (-not $scrolls) { $right = [Math]::Min($right, $r) }
        }
        foreach ($c in @($n.children)) { if ($c) { Walk $c $right } }
    }
    Walk $j.accessibility ([double]::MaxValue)
    return @($script:ovf)
}
