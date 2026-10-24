#!/usr/bin/env python3
"""Owned GTK fixture advertising real text/uri-list drag data."""
import sys
from pathlib import Path
import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk,Gdk
window=Gtk.Window(title='zgx file source');window.set_default_size(240,120)
button=Gtk.Button(label='Drag two fixture files');window.add(button)
button.drag_source_set(Gdk.ModifierType.BUTTON1_MASK,[Gtk.TargetEntry.new('text/uri-list',0,0)],Gdk.DragAction.COPY)
uris=[Path(value).resolve().as_uri() for value in sys.argv[1:]]
def data(widget,context,selection,info,time):
    selection.set_uris(uris)
    print('SOURCE_DATA',uris,flush=True)
button.connect('drag-data-get',data)
button.connect('drag-end',lambda *args: print('SOURCE_END',flush=True))
window.connect('destroy',Gtk.main_quit);window.show_all();Gtk.main()
