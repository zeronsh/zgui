// Field is a focused text field left alone (the suite's field scene): MyGo's
// own TextInput, focused as the window appears, its caret blinking as
// MyGo's does (530 ms each way).
//
//	ZGX_SECONDS=30 bin/field
package main

import (
	"fmt"
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

type scene struct{ value string }

func (s *scene) view(c *ui.Context) {
	ui.Box(c).Fill().Background(adapter.RGB(w.BlinkBackground)).Font(w.Font).FontSize(14).Padding(40).Children(func() {
		ui.TextInput(c, &s.value).Width(400).AutoFocus()
	})
}

func main() {
	s := &scene{value: "Reading, not typing"}
	adapter.Window("MyGo field", w.Width, w.Height, s.view, func(win *mygo.Window) {
		adapter.Ticks(win, w.SecondsFromEnv(), false, nil, nil, func() {
			fmt.Fprintln(os.Stderr, "workload_ticks=0")
		})
	})
}
