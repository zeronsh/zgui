// Blink is a caret blinking in a still window (see the workload's
// blink.go): MyGo draws the window again each phase, when it is
// invalidated.
//
//	ZGX_SECONDS=10 bin/blink
package main

import (
	"fmt"
	"os"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

type scene struct {
	shown  bool
	frames uint64
}

func (s *scene) view(c *ui.Context) {
	s.frames++
	ui.Box(c).Fill().Background(adapter.RGB(w.BlinkBackground)).Children(func() {
		if s.shown {
			ui.Box(c).Absolute().Left(w.BlinkCaret[0]).Top(w.BlinkCaret[1]).Size(w.BlinkCaret[2], w.BlinkCaret[3]).
				Background(adapter.RGB(w.BlinkCaretColor))
		}
	})
}

func main() {
	s := &scene{shown: true}
	adapter.Window("MyGo blink", w.Width, w.Height, s.view, func(win *mygo.Window) {
		seconds := w.SecondsFromEnv()
		start := time.Now()
		go func() {
			for {
				time.Sleep(w.BlinkPhase)
				if seconds > 0 && time.Since(start).Seconds() >= seconds {
					mygo.RunOnMain(func() { fmt.Fprintf(os.Stderr, "blink_frames=%d\n", s.frames) })
					mygo.App.Quit()
					return
				}
				mygo.RunOnMain(func() {
					s.shown = !s.shown
					win.Invalidate()
				})
			}
		}()
	})
}
