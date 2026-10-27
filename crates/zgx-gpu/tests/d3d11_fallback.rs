//! Where Direct3D 11 is asked for but no device fit for it opens (none made,
//! a software adapter, no shared fences), or Direct3D 12 is asked for over
//! it, the context opens Direct3D 12 and draws as without the feature. The
//! choice is read once a process: each case runs in a process of its own,
//! its failure forced through `ZGX_D3D11_FAIL` (a debug builds' hook).
#[cfg(windows)]
use zgx::scene::{Color, Layout, NodeKind, Scene, Style};
#[cfg(windows)]
use zgx_gpu::GpuRenderer;

/// Set in each case's process: what it reports.
#[cfg(windows)]
const CHILD: &str = "ZGX_D3D11_FALLBACK_CHILD";

/// This process's backend and its frame's pixel, as `backend r g b a`.
#[cfg(windows)]
fn draw() -> String {
    let mut scene = Scene::new(64., 64.);
    scene.set_kind(scene.root(), NodeKind::Container(Layout::Overlay));
    scene.append(
        scene.root(),
        NodeKind::Rect(Color(220, 30, 40, 255)),
        Style {
            width: Some(64.),
            height: Some(64.),
            ..Default::default()
        },
    );
    let mut gpu = GpuRenderer::new(64, 64).unwrap();
    let damage = scene.flush().damage;
    gpu.render(&scene, &damage).unwrap();
    let pixels = gpu.readback().unwrap();
    let at = (32 * 64 + 32) * 4;
    format!(
        "{:?} {} {} {} {}",
        gpu.adapter_info().backend,
        pixels[at],
        pixels[at + 1],
        pixels[at + 2],
        pixels[at + 3]
    )
}

#[cfg(windows)]
fn main() {
    if std::env::var_os(CHILD).is_some() {
        println!("{}", draw());
        return;
    }
    let case = |env: &[(&str, &str)]| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.env(CHILD, "1");
        for name in ["ZGX_D3D11", "ZGX_D3D12", "ZGX_D3D11_FAIL"] {
            command.env_remove(name);
        }
        command.envs(env.iter().copied());
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{env:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        (stdout.trim().to_owned(), stderr)
    };
    let red = "220 30 40 255";
    // Asked for and fit: Direct3D 11, wgpu's no-op shell.
    let (drawn, _) = case(&[("ZGX_D3D11", "1")]);
    assert_eq!(drawn, format!("Noop {red}"), "Direct3D 11 asked for");
    // Not asked for, or Direct3D 12 asked for over it.
    for env in [
        &[][..],
        &[("ZGX_D3D11", "0")],
        &[("ZGX_D3D11", "1"), ("ZGX_D3D12", "1")],
    ] {
        let (drawn, _) = case(env);
        assert_eq!(drawn, format!("Dx12 {red}"), "{env:?}");
    }
    // Asked for, and unfit: Direct3D 12, said once on stderr.
    if cfg!(debug_assertions) {
        for failure in ["device", "software", "fences"] {
            let (drawn, said) = case(&[("ZGX_D3D11", "1"), ("ZGX_D3D11_FAIL", failure)]);
            assert_eq!(drawn, format!("Dx12 {red}"), "{failure}");
            assert_eq!(
                said.matches("Direct3D 11 unavailable").count(),
                1,
                "{failure}: {said}"
            );
        }
    }
    println!("d3d11_fallback: every case drew on the backend it should");
}

#[cfg(not(windows))]
fn main() {}
