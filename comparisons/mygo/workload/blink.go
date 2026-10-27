package workload

import "time"

// The blink scene (zgx_workload::blink): a 2 × 20 point caret shown and
// hidden every 530 ms in a still window.
const (
	BlinkPhase      = 530 * time.Millisecond
	BlinkCaretColor = 0xe5edf7
	BlinkBackground = 0x10141c
)

// BlinkCaret is the caret's x, y, width and height.
var BlinkCaret = [4]float32{40, 40, 2, 20}
