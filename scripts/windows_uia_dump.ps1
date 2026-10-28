# Print a window's UI Automation tree as screen readers (Narrator) see it:
# one line per element of the control view, with its control type, name,
# value, selection, heading level and text selection where it has them.
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows_uia_dump.ps1 -Hwnd 123456
param([Parameter(Mandatory = $true)][long]$Hwnd, [int]$Depth = 12, [switch]$Rects, [switch]$Raw)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$HeadingLevel = [System.Windows.Automation.AutomationProperty]::LookupById(30173)
# The control view is what screen readers walk; `-Raw` shows every element.
$walker = if ($Raw) { [System.Windows.Automation.TreeWalker]::RawViewWalker } else { [System.Windows.Automation.TreeWalker]::ControlViewWalker }

function Describe($element) {
    $info = $element.Current
    $line = "{0} name={1}" -f $info.ControlType.ProgrammaticName.Replace("ControlType.", ""), (ConvertTo-Json $info.Name -Compress)
    $pattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $line += " value=" + (ConvertTo-Json $pattern.Current.Value -Compress)
    }
    if ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
        $line += " toggle=" + $pattern.Current.ToggleState
    }
    if ($element.TryGetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
        $line += " range=" + $pattern.Current.Value
    }
    if ($element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
        $line += " selected=" + $pattern.Current.IsSelected
    }
    # HeadingLevel (30173), which heading navigation goes by: "Level2".
    $heading = $element.GetCurrentPropertyValue($HeadingLevel, $true)
    if ($heading -is [int] -or ($heading -is [enum] -and "$heading" -ne "None")) {
        $line += " heading=" + $heading
    }
    if ($element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        $selected = @($pattern.GetSelection() | ForEach-Object { $_.GetText(-1) }) -join "|"
        $line += " text-selection=" + (ConvertTo-Json $selected -Compress)
    }
    if ($Rects) {
        $r = $info.BoundingRectangle
        # An element with no bounds (a scrollbar not shown) reports Rect.Empty, whose
        # coordinates are infinite.
        if ($r.IsEmpty) { $line += " rect=none" }
        else { $line += " rect=" + [int]$r.X + "," + [int]$r.Y + "," + [int]$r.Width + "," + [int]$r.Height }
    }
    if ($info.HasKeyboardFocus) { $line += " focused" }
    if (-not $info.IsEnabled) { $line += " disabled" }
    return $line
}

function Walk($element, $level) {
    "{0}{1}" -f ("  " * $level), (Describe $element)
    if ($level -ge $Depth) { return }
    $child = $walker.GetFirstChild($element)
    while ($child -ne $null) {
        Walk $child ($level + 1)
        $child = $walker.GetNextSibling($child)
    }
}

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Walk $root 0
