#!/usr/bin/env python3
"""Exercise native Wayland file dialogs through a private GTK portal."""
from sway_host import wait_for_openbox
import argparse
import json
import os
import pathlib
import re
import select
import shutil
import signal
import subprocess
import tempfile
import time

from platform_smoke import stop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--tools-root', type=pathlib.Path, help='Private extracted Zenity/GTK runtime for prompt verification')
    parser.add_argument('--close-picker', action='store_true', help='Close owner while portal picker is open; assert both disappear without exiting app')
    parser.add_argument('--escape-cancel', action='store_true', help='Probe GTK Escape termination separately from its Cancel button')
    parser.add_argument('--native-prompts', action='store_true', help='Verify prompt and URL using installed system Zenity')
    args = parser.parse_args()
    for name in ('Xvfb', 'openbox', 'sway', 'swaymsg', 'dbus-daemon', 'xdotool', 'xwininfo', 'grim'):
        if not shutil.which(name):
            parser.error('Missing executable: ' + name)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output/'result.json').unlink(missing_ok=True)
    app = portal = gtk_portal = compositor = wm = xvfb = bus = monitor = None
    with tempfile.TemporaryDirectory(prefix='zgx-wayland-ime-') as private:
        env = dict(os.environ, XDG_RUNTIME_DIR=private, XDG_CONFIG_HOME=private+'/config',
                   XDG_CACHE_HOME=private+'/cache', XDG_DATA_HOME=private+'/data',
                   WLR_BACKENDS='x11', WLR_X11_OUTPUTS='1', WLR_RENDERER='pixman', LANG='C.UTF-8')
        for name in ('DISPLAY', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'I3SOCK', 'IBUS_ADDRESS', 'XMODIFIERS', 'GTK_IM_MODULE', 'QT_IM_MODULE', 'QT_IM_MODULES', 'SDL_IM_MODULE', 'AT_SPI_BUS_ADDRESS', 'FCITX_DBUS_ADDRESS'):
            env.pop(name, None)
        config = pathlib.Path(private, 'sway.conf')
        config.write_text('xwayland disable\nseat seat0 fallback true\noutput * mode 1200x800\ndefault_border none\nfor_window [title="zgx platform services"] floating enable\n')
        (args.output/'sway.conf').write_text(config.read_text())
        bus_config = pathlib.Path(private, 'dbus.conf')
        # No activation directories: desktop portals must not mount into this
        # disposable runtime or start unrelated services during the probe.
        bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir=' + private + '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        def launch(name, command, process_env=env, **kwargs):
            with (args.output/(name+'.log')).open('w') as log:
                return subprocess.Popen(command, env=process_env, stderr=log, stdout=kwargs.pop('stdout', log), **kwargs)
        try:
            xvfb = launch('xvfb', ['Xvfb','-displayfd','1','-screen','0','1400x1000x24','-ac'], stdout=subprocess.PIPE, text=True)
            if not select.select([xvfb.stdout], [], [], 10)[0]: raise RuntimeError('Xvfb timeout')
            env['DISPLAY'] = ':'+xvfb.stdout.readline().strip()
            bus = launch('dbus', ['dbus-daemon','--config-file='+str(bus_config),'--nofork','--print-address=1'], stdout=subprocess.PIPE, text=True, start_new_session=True)
            if not select.select([bus.stdout], [], [], 10)[0]: raise RuntimeError('D-Bus timeout')
            env['DBUS_SESSION_BUS_ADDRESS'] = bus.stdout.readline().strip()
            wm = launch('openbox', ['openbox'])
            wait_for_openbox(env, args.output, wm)
            compositor = launch('sway', ['sway','--unsupported-gpu','--config',str(config)])
            deadline = time.monotonic()+10
            while True:
                sockets = list(pathlib.Path(private).glob('sway-ipc*.sock'))
                displays = [p for p in pathlib.Path(private).glob('wayland-*') if p.suffix != '.lock']
                if sockets and displays: break
                if time.monotonic()>deadline or compositor.poll() is not None: raise RuntimeError('Sway failed')
                time.sleep(.05)
            env.update(WAYLAND_DISPLAY=displays[0].name, SWAYSOCK=str(sockets[0]))
            deadline = time.monotonic()+8
            while True:
                tree = subprocess.check_output(['xwininfo','-root','-tree'], env=env, text=True)
                match = re.search(r'(0x[0-9a-f]+) "wlroots - X11-1".*1200x800', tree)
                if match: break
                if time.monotonic()>deadline: raise RuntimeError('Sway host window missing')
                time.sleep(.05)
            window = match.group(1)
            def xdo(*parts):
                subprocess.run(['xdotool', *map(str, parts)], env=env, check=True)
            xdo('windowfocus','--sync',window)
            native = dict(env, WAYLAND_DEBUG='1', WINIT_UNIX_BACKEND='wayland')
            native.pop('DISPLAY', None)
            native['XDG_CURRENT_DESKTOP'] = 'sway'
            if args.tools_root or args.native_prompts:
                if args.tools_root:
                    root=args.tools_root.resolve()
                    native['PATH']=str(root/'usr/bin')+':'+native.get('PATH','')
                    native['LD_LIBRARY_PATH']=str(root/'usr/lib/x86_64-linux-gnu')+':'+native.get('LD_LIBRARY_PATH','')
                    native['XDG_DATA_DIRS']=str(root/'usr/share')+':/usr/local/share:/usr/share'
                native['GSK_RENDERER']='cairo'
                applications=pathlib.Path(private,'data/applications'); applications.mkdir(parents=True)
                handler=pathlib.Path(private,'record-url')
                handler.write_text("#!/bin/sh\nprintf '%s\\n' \"$1\" >> "+str(args.output.resolve()/'url-handler.log')+'\n')
                handler.chmod(0o700)
                (applications/'zgx-url.desktop').write_text('[Desktop Entry]\nType=Application\nName=zgx URL test\nExec='+str(handler)+' %u\nMimeType=x-scheme-handler/zgx-smoke;\nNoDisplay=true\n')
                pathlib.Path(private,'config').mkdir(exist_ok=True)
                pathlib.Path(private,'config/mimeapps.list').write_text('[Default Applications]\nx-scheme-handler/zgx-smoke=zgx-url.desktop\n')
                (applications/'mimeinfo.cache').write_text('[MIME Cache]\nx-scheme-handler/zgx-smoke=zgx-url.desktop;\n')
            portals = pathlib.Path(private, 'config/xdg-desktop-portal')
            portals.mkdir(parents=True, exist_ok=True)
            (portals/'portals.conf').write_text('[preferred]\ndefault=gtk\n')
            gtk_portal = launch('gtk-portal', ['/usr/libexec/xdg-desktop-portal-gtk'], process_env=native)
            portal = launch('portal', ['/usr/libexec/xdg-desktop-portal'], process_env=native)
            fixture = args.output.resolve()/'fixture.txt'
            fixture.write_text('native portal selection fixture\n')
            folder=args.output.resolve()/'chosen-folder'; folder.mkdir(exist_ok=True)
            native['ZGX_DIALOG_DIRECTORY']=str(folder)
            with (args.output/'protocol.log').open('w') as protocol, (args.output/'application.log').open('w') as log:
                monitor = launch('request-monitor', ['dbus-monitor', '--session', "interface='org.freedesktop.portal.Request'"], process_env=native)
                app = subprocess.Popen([str(args.binary.resolve()), '--smoke'] + (['--close-picker'] if args.close_picker else []), env=native, stdout=log, stderr=protocol)
                def wait_log(token, seconds=12):
                    deadline=time.monotonic()+seconds
                    while token not in (args.output/'application.log').read_text():
                        if time.monotonic()>deadline or app.poll() is not None:
                            subprocess.run(['grim',str(args.output.resolve()/'timeout.png')],env=env,check=False)
                            raise RuntimeError('Missing application result: '+token)
                        time.sleep(.05)
                def find_dialog(title="Open fixture",seconds=30):
                    deadline=time.monotonic()+seconds
                    while time.monotonic()<deadline:
                        tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True))
                        pending=[tree]
                        while pending:
                            node=pending.pop(); pending.extend(node.get('nodes',[])+node.get('floating_nodes',[]))
                            if node.get('name')==title: return node
                        time.sleep(.05)
                    (args.output/'timeout-tree.json').write_text(json.dumps(tree,indent=2))
                    subprocess.run(['grim',str(args.output.resolve()/'timeout.png')],env=env,check=False)
                    raise RuntimeError('Native portal dialog never appeared: '+title)
                wait_log('READY')
                dialog=find_dialog()
                (args.output/'dialog.json').write_text(json.dumps(dialog,indent=2)+'\n')
                if args.close_picker:
                    subprocess.run(['swaymsg','[title="^zgx platform services$"]','kill'],env=env,check=True,stdout=subprocess.DEVNULL)
                    wait_log('CLOSED')
                    deadline=time.monotonic()+5
                    while True:
                        tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True));pending=[tree];names=[]
                        while pending:
                            node=pending.pop();pending.extend(node.get('nodes',[])+node.get('floating_nodes',[]));names.append(node.get('name'))
                        if 'Open fixture' not in names and 'zgx platform services' not in names:break
                        if time.monotonic()>deadline:raise RuntimeError('Closing owner retained native parent or portal picker')
                        time.sleep(.05)
                    assert 'zgx dialog ownership keeper' in names
                    assert app.poll() is None, 'App exited instead of releasing the dialog owner'
                    assert 'DIALOG open ' not in (args.output/'application.log').read_text(), 'Closed owner received picker result'
                    assert 'member=Close' in (args.output/'request-monitor.log').read_text(), 'No portal Request.Close observed'
                    subprocess.run(['grim',str(args.output.resolve()/'owner-closed.png')],env=env,check=True)
                    subprocess.run(['swaymsg','[pid='+str(app.pid)+']','kill'],env=env,check=True,stdout=subprocess.DEVNULL)
                    app.wait(timeout=5);assert app.returncode==0
                    (args.output/'result.json').write_text(json.dumps(dict(result='pass',owner_close_releases_parent=True,portal_close_observed=True,closed_callback_suppressed=True,application_survives_owner=True),indent=2)+'\n')
                    return
                subprocess.run(['grim',str(args.output.resolve()/'dialog.png')],env=env,check=True)
                subprocess.run(['swaymsg','[con_id='+str(dialog['id'])+']','focus'],env=env,check=True,stdout=subprocess.DEVNULL)
                xdo('windowactivate','--sync',window)
                xdo('windowfocus','--sync',window)
                xdo('key','--clearmodifiers','ctrl+l')
                time.sleep(.2)
                xdo('type','--clearmodifiers','--delay',1,str(fixture))
                xdo('key','--clearmodifiers','Return')
                time.sleep(.5)
                subprocess.run(['grim',str(args.output.resolve()/'entered.png')],env=env,check=True)
                xdo('key','--clearmodifiers','Return')
                wait_log('DIALOG open Ok(Some(')
                trace=(args.output/'application.log').read_text()
                assert str(fixture) in trace, trace
                subprocess.run(['grim',str(args.output.resolve()/'selected.png')],env=env,check=True)
                def click_app(x,y):
                    tree=json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True))
                    pending=[tree]
                    while pending:
                        node=pending.pop(); pending.extend(node.get('nodes',[])+node.get('floating_nodes',[]))
                        if node.get('pid')==app.pid and node.get('name')=='zgx platform services':
                            rect=node['rect']; xdo('mousemove','--window',window,rect['x']+x,rect['y']+y,'click',1); return
                    raise RuntimeError('Application window missing')
                # Select an actual folder through the same native portal.
                click_app(70,128)
                pick=find_dialog('Choose folder')
                time.sleep(.4)
                subprocess.run(['grim',str(args.output.resolve()/'folder-entered.png')],env=env,check=True)
                rect=pick['rect']; xdo('mousemove','--window',window,rect['x']+rect['width']-45,rect['y']+24,'click',1)
                wait_log('DIALOG folder Ok(Some(')
                assert str(folder) in (args.output/'application.log').read_text()
                # Native save selection must not create the destination itself.
                destination=folder/'saved.txt'
                click_app(65,176)
                pick=find_dialog('Save fixture')
                time.sleep(.4)
                xdo('key','--clearmodifiers','Return')
                wait_log('DIALOG save Ok(Some(')
                assert str(destination) in (args.output/'application.log').read_text()
                assert not destination.exists(), 'Selecting save path unexpectedly wrote a file'
                # Open through the rendered File menu and cancel the real picker.
                # The menu opens a frame after the click: a Return before it
                # does goes elsewhere, so a picker that never comes is asked
                # for again, from a closed menu.
                for attempt in range(3):
                    click_app(40,35); time.sleep(.3)
                    xdo('key','--clearmodifiers','Return')
                    try:
                        cancel_dialog=find_dialog(seconds=5 if attempt<2 else 30)
                        break
                    except RuntimeError:
                        if attempt==2: raise
                        xdo('key','--clearmodifiers','Escape'); time.sleep(.3)
                time.sleep(.2)
                if args.escape_cancel:xdo('key','--clearmodifiers','Escape')
                else:
                    rect=cancel_dialog['rect'];xdo('mousemove','--window',window,rect['x']+45,rect['y']+24,'click',1)
                wait_log('DIALOG open Err(BackendUnavailable)' if args.escape_cancel else 'DIALOG open Ok(None)')
                if args.tools_root or args.native_prompts:
                    time.sleep(.3)
                    click_app(65,224)
                    prompt=find_dialog('Confirm native action')
                    subprocess.run(['grim',str(args.output.resolve()/'prompt.png')],env=env,check=True)
                    xdo('key','--clearmodifiers','Return')
                    wait_log('PROMPT Ok(Ok)')
                    click_app(65,272)
                    wait_log('URL Ok(())')
                    deadline=time.monotonic()+5
                    while not (args.output/'url-handler.log').exists():
                        if time.monotonic()>deadline:raise RuntimeError('URL handler never received URI')
                        time.sleep(.05)
                    assert (args.output/'url-handler.log').read_text().strip()=='zgx-smoke:accepted'
                subprocess.run(['swaymsg','[pid='+str(app.pid)+']','kill'],env=env,check=True,stdout=subprocess.DEVNULL)
                app.wait(timeout=5)
                assert app.returncode==0
                assert 'zxdg_exporter_v2' in (args.output/'protocol.log').read_text()
                assert '.export_toplevel(' in (args.output/'protocol.log').read_text(), 'Missing Wayland parent export'
                (args.output/'result.json').write_text(json.dumps(dict(result='pass',backend='native Wayland / XDG Desktop Portal GTK',selected=str(fixture),folder=str(folder),save_destination=str(destination),save_did_not_write=True,menu_cancel=not args.escape_cancel,escape_termination_probe=args.escape_cancel,client_display_unset=True,prompt=bool(args.tools_root or args.native_prompts),url_handler=bool(args.tools_root or args.native_prompts)),indent=2)+'\n')
        finally:
            for process in (app,monitor,portal,gtk_portal,compositor,wm,xvfb,bus): stop(process)

if __name__=='__main__': main()
