use alloc::{string::String, vec::Vec};
use core::ffi::CStr;
use std::path::PathBuf;

use crate::auxil::dxgi::result::HResult;
use thiserror::Error;
use windows::{
    Win32::Graphics::Direct3D::{D3D_SHADER_MACRO, Dxc, Fxc, ID3DBlob},
    core::{Interface, PCSTR, PCWSTR},
};

pub(super) enum CompilerContainer {
    Fxc(CompilerFxc),
    DynamicDxc(CompilerDynamicDxc),
    #[cfg_attr(not(static_dxc), allow(unused))]
    StaticDxc(CompilerStaticDxc),
}

pub(super) struct CompilerFxc {
    fxc: FxcLib,
}

pub(super) struct CompilerDynamicDxc {
    max_shader_model: wgt::DxcShaderModel,
    compiler: Dxc::IDxcCompiler3,
    /// zgx patch: what tells this DLL from another for the disk cache (its
    /// version, and its file's size and time: the DLL is the app's own).
    identity: String,
    // Has to be held onto for the lifetime of the device otherwise shaders will fail to compile.
    // Only needed when using dynamic linking.
    _dxc: DxcLib,
}

pub(super) struct CompilerStaticDxc {
    max_shader_model: wgt::DxcShaderModel,
    compiler: Dxc::IDxcCompiler3,
    identity: String,
}

#[derive(Debug, Error)]
pub(super) enum GetContainerError {
    #[error(transparent)]
    Device(#[from] crate::DeviceError),
    #[error("Failed to load {0}: {1}")]
    FailedToLoad(&'static str, libloading::Error),
}

impl CompilerContainer {
    pub(super) fn new_fxc() -> Result<Self, GetContainerError> {
        FxcLib::new_dynamic().map(|fxc| Self::Fxc(CompilerFxc { fxc }))
    }

    pub(super) fn new_dynamic_dxc(dxc_path: PathBuf) -> Result<Self, GetContainerError> {
        let file = std::fs::metadata(&dxc_path).ok().map(|meta| {
            let time = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{} {time}", meta.len())
        });
        let dxc = DxcLib::new_dynamic(dxc_path)
            .map_err(|e| GetContainerError::FailedToLoad("dxcompiler.dll", e))?;

        let compiler = dxc.create_instance::<Dxc::IDxcCompiler3>()?;

        let (mut major, mut minor) = (1, 0);
        // DXC 1.0 didn't support this. If the cast fails assume it is DXC 1.0.
        if let Ok(version_info) = compiler.cast::<Dxc::IDxcVersionInfo>() {
            unsafe {
                version_info.GetVersion(&mut major, &mut minor).unwrap();
            }
        }

        Ok(Self::DynamicDxc(CompilerDynamicDxc {
            max_shader_model: wgt::DxcShaderModel::from_dxc_version(major, minor),
            compiler,
            // A DLL found by name on the search path has no file to read:
            // its output isn't kept.
            identity: file.map_or(String::new(), |file| format!("dxc {major}.{minor} {file}")),
            _dxc: dxc,
        }))
    }

    /// Creates a [`CompilerContainer`] that delegates to the statically-linked version of DXC.
    pub(super) fn new_static_dxc() -> Result<CompilerContainer, crate::DeviceError> {
        #[cfg(static_dxc)]
        {
            unsafe {
                let compiler = dxc_create_instance::<Dxc::IDxcCompiler3>(|clsid, iid, ppv| {
                    windows_core::HRESULT(mach_dxcompiler_rs::DxcCreateInstance(
                        clsid.cast(),
                        iid.cast(),
                        ppv,
                    ))
                })?;

                Ok(CompilerContainer::StaticDxc(CompilerStaticDxc {
                    max_shader_model: wgt::DxcShaderModel::V6_7,
                    compiler,
                    identity: String::new(),
                }))
            }
        }
        #[cfg(not(static_dxc))]
        {
            panic!(
                "Attempted to create a static DXC shader compiler, but the static-dxc feature was not enabled"
            )
        }
    }

    pub(super) fn max_shader_model(&self) -> Option<wgt::DxcShaderModel> {
        match self {
            CompilerContainer::Fxc(..) => None,
            CompilerContainer::DynamicDxc(CompilerDynamicDxc {
                max_shader_model, ..
            })
            | CompilerContainer::StaticDxc(CompilerStaticDxc {
                max_shader_model, ..
            }) => Some(max_shader_model.clone()),
        }
    }

    pub(super) fn compile(
        &self,
        device: &super::Device,
        source: &str,
        source_name: Option<&CStr>,
        raw_ep: &str,
        stage_bit: wgt::ShaderStages,
        full_stage: &str,
    ) -> Result<super::CompiledShader, crate::PipelineError> {
        match self {
            CompilerContainer::Fxc(CompilerFxc { fxc }) => compile_fxc(
                device,
                source,
                source_name,
                raw_ep,
                stage_bit,
                full_stage,
                fxc,
            ),
            CompilerContainer::DynamicDxc(CompilerDynamicDxc {
                compiler, identity, ..
            })
            | CompilerContainer::StaticDxc(CompilerStaticDxc {
                compiler, identity, ..
            }) => compile_dxc(
                device,
                source,
                source_name,
                raw_ep,
                stage_bit,
                full_stage,
                compiler,
                identity,
            ),
        }
    }
}

type D3DCompileFn = unsafe extern "system" fn(
    psrcdata: *const core::ffi::c_void,
    srcdatasize: usize,
    psourcename: PCSTR,
    pdefines: *const D3D_SHADER_MACRO,
    pinclude: *mut core::ffi::c_void,
    pentrypoint: PCSTR,
    ptarget: PCSTR,
    flags1: u32,
    flags2: u32,
    ppcode: *mut *mut core::ffi::c_void,
    pperrormsgs: *mut *mut core::ffi::c_void,
) -> windows_core::HRESULT;

#[derive(Debug)]
struct FxcLib {
    // `d3dcompile_fn` points into `_lib`, so `_lib` must be held for as long
    // as we want to keep compiling shaders with FXC.
    _lib: crate::dx12::DynLib,
    d3dcompile_fn: D3DCompileFn,
}

impl FxcLib {
    const PATH: &str = "d3dcompiler_47.dll";

    fn new_dynamic() -> Result<Self, GetContainerError> {
        unsafe {
            let lib = crate::dx12::DynLib::new(Self::PATH)
                .map_err(|e| GetContainerError::FailedToLoad(FxcLib::PATH, e))?;
            let d3dcompile_fn: D3DCompileFn = *lib.get::<D3DCompileFn>(c"D3DCompile".to_bytes())?;

            Ok(Self {
                _lib: lib,
                d3dcompile_fn,
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compile(
        &self,
        source: &str,
        source_name: Option<&CStr>,
        raw_ep: &str,
        full_stage: &str,
        compile_flags: u32,
        shader_data: &mut Option<ID3DBlob>,
        error: &mut Option<ID3DBlob>,
    ) -> Result<windows_core::Result<()>, crate::DeviceError> {
        unsafe {
            let raw_ep = alloc::ffi::CString::new(raw_ep).unwrap();
            let full_stage = alloc::ffi::CString::new(full_stage).unwrap();

            // If no name has been set, D3DCompile wants the null pointer.
            let source_name = source_name
                .map(|cstr| cstr.as_ptr().cast())
                .unwrap_or(core::ptr::null());

            let shader_data: *mut Option<ID3DBlob> = shader_data;
            let error: *mut Option<ID3DBlob> = error;

            {
                profiling::scope!("Fxc::D3DCompile");
                Ok((self.d3dcompile_fn)(
                    source.as_ptr().cast(),
                    source.len(),
                    PCSTR(source_name),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                    PCSTR(raw_ep.as_ptr().cast()),
                    PCSTR(full_stage.as_ptr().cast()),
                    compile_flags,
                    0,
                    shader_data.cast(),
                    error.cast(),
                )
                .ok())
            }
        }
    }
}

fn compile_fxc(
    device: &super::Device,
    source: &str,
    source_name: Option<&CStr>,
    raw_ep: &str,
    stage_bit: wgt::ShaderStages,
    full_stage: &str,
    fxc: &FxcLib,
) -> Result<super::CompiledShader, crate::PipelineError> {
    profiling::scope!("compile_fxc");
    let mut compile_flags = Fxc::D3DCOMPILE_ENABLE_STRICTNESS;
    if device
        .shared
        .private_caps
        .instance_flags
        .contains(wgt::InstanceFlags::DEBUG)
    {
        compile_flags |= Fxc::D3DCOMPILE_DEBUG | Fxc::D3DCOMPILE_SKIP_OPTIMIZATION;
    }

    // zgx patch: compiled at an earlier launch, read back (`shader_disk_cache`).
    let cache_key = super::shader_disk_cache::key(compile_flags, full_stage, raw_ep, source);
    if let Some(code) = super::shader_disk_cache::read(&cache_key) {
        return Ok(super::CompiledShader::Precompiled(code));
    }

    let mut shader_data = None;
    let mut error = None;
    let hr = fxc.compile(
        source,
        source_name,
        raw_ep,
        full_stage,
        compile_flags,
        &mut shader_data,
        &mut error,
    )?;

    match hr {
        Ok(()) => {
            let shader_data: ID3DBlob = shader_data.unwrap();
            // SAFETY: the blob's own buffer, read while the blob lives.
            let code = unsafe {
                core::slice::from_raw_parts(
                    shader_data.GetBufferPointer().cast::<u8>(),
                    shader_data.GetBufferSize(),
                )
            };
            super::shader_disk_cache::write(&cache_key, code);
            Ok(super::CompiledShader::Fxc(shader_data))
        }
        Err(e) => {
            let mut full_msg = format!("FXC D3DCompile error ({e})");
            if let Some(error) = error {
                use core::fmt::Write as _;
                let message = unsafe {
                    core::slice::from_raw_parts(
                        error.GetBufferPointer().cast(),
                        error.GetBufferSize(),
                    )
                };
                let _ = write!(full_msg, ": {}", String::from_utf8_lossy(message));
            }
            Err(crate::PipelineError::Linkage(stage_bit, full_msg))
        }
    }
}

trait DxcObj: Interface {
    const CLSID: windows::core::GUID;
}
impl DxcObj for Dxc::IDxcCompiler3 {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcCompiler;
}
impl DxcObj for Dxc::IDxcUtils {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcUtils;
}
impl DxcObj for Dxc::IDxcValidator {
    const CLSID: windows::core::GUID = Dxc::CLSID_DxcValidator;
}

#[derive(Debug)]
struct DxcLib {
    lib: crate::dx12::DynLib,
}

impl DxcLib {
    fn new_dynamic(lib_path: PathBuf) -> Result<Self, libloading::Error> {
        unsafe { crate::dx12::DynLib::new(lib_path).map(|lib| Self { lib }) }
    }

    pub fn create_instance<T: DxcObj>(&self) -> Result<T, crate::DeviceError> {
        unsafe {
            type DxcCreateInstanceFn = unsafe extern "system" fn(
                rclsid: *const windows_core::GUID,
                riid: *const windows_core::GUID,
                ppv: *mut *mut core::ffi::c_void,
            )
                -> windows_core::HRESULT;

            let func: libloading::Symbol<DxcCreateInstanceFn> =
                self.lib.get(c"DxcCreateInstance".to_bytes())?;
            dxc_create_instance::<T>(|clsid, iid, ppv| func(clsid, iid, ppv))
        }
    }
}

/// Invokes the provided library function to create a DXC object.
unsafe fn dxc_create_instance<T: DxcObj>(
    f: impl Fn(
        *const windows_core::GUID,
        *const windows_core::GUID,
        *mut *mut core::ffi::c_void,
    ) -> windows_core::HRESULT,
) -> Result<T, crate::DeviceError> {
    let mut result__ = None;
    f(&T::CLSID, &T::IID, <*mut _>::cast(&mut result__))
        .ok()
        .into_device_result("DxcCreateInstance")?;
    result__.ok_or(crate::DeviceError::Unexpected)
}

/// Owned PCWSTR
#[allow(clippy::upper_case_acronyms)]
struct OPCWSTR {
    inner: Vec<u16>,
}

impl OPCWSTR {
    fn new(s: &str) -> Self {
        let mut inner: Vec<_> = s.encode_utf16().collect();
        inner.push(0);
        Self { inner }
    }

    fn ptr(&self) -> PCWSTR {
        PCWSTR(self.inner.as_ptr())
    }
}

fn get_output<T: Interface>(
    res: &Dxc::IDxcResult,
    kind: Dxc::DXC_OUT_KIND,
) -> Result<T, crate::DeviceError> {
    let mut result__: Option<T> = None;
    unsafe { res.GetOutput::<T>(kind, &mut None, <*mut _>::cast(&mut result__)) }
        .into_device_result("GetOutput")?;
    result__.ok_or(crate::DeviceError::Unexpected)
}

fn as_err_str(blob: &Dxc::IDxcBlobUtf8) -> Result<&str, crate::DeviceError> {
    let ptr = unsafe { blob.GetStringPointer() };
    let len = unsafe { blob.GetStringLength() };
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(ptr.0, len) })
        .map_err(|_| crate::DeviceError::Unexpected)
}

#[allow(clippy::too_many_arguments)]
fn compile_dxc(
    device: &crate::dx12::Device,
    source: &str,
    source_name: Option<&CStr>,
    raw_ep: &str,
    stage_bit: wgt::ShaderStages,
    full_stage: &str,
    compiler: &Dxc::IDxcCompiler3,
    identity: &str,
) -> Result<crate::dx12::CompiledShader, crate::PipelineError> {
    profiling::scope!("compile_dxc");
    // zgx patch: compiled at an earlier launch by the same DLL, read back
    // (`shader_disk_cache`); the flags stand for the debug arguments below.
    let debug = device
        .shared
        .private_caps
        .instance_flags
        .contains(wgt::InstanceFlags::DEBUG);
    let f16 = device.features.contains(wgt::Features::SHADER_F16);
    let cache_key = (!identity.is_empty()).then(|| {
        super::shader_disk_cache::key(
            u32::from(debug) | u32::from(f16) << 1,
            &format!("{identity}\n{full_stage}"),
            raw_ep,
            source,
        )
    });
    if let Some(code) = cache_key
        .as_deref()
        .and_then(super::shader_disk_cache::read)
    {
        return Ok(crate::dx12::CompiledShader::Precompiled(code));
    }

    let source_name = source_name.and_then(|cstr| cstr.to_str().ok());

    let source_name = source_name.map(OPCWSTR::new);
    let raw_ep = OPCWSTR::new(raw_ep);
    let full_stage = OPCWSTR::new(full_stage);

    let mut compile_args = arrayvec::ArrayVec::<PCWSTR, 13>::new_const();

    if let Some(source_name) = source_name.as_ref() {
        compile_args.push(source_name.ptr())
    }

    compile_args.extend([
        windows::core::w!("-E"),
        raw_ep.ptr(),
        windows::core::w!("-T"),
        full_stage.ptr(),
        windows::core::w!("-HV"),
        windows::core::w!("2018"), // Use HLSL 2018, Naga doesn't supported 2021 yet.
        windows::core::w!("-no-warnings"),
        Dxc::DXC_ARG_ENABLE_STRICTNESS,
    ]);

    if device
        .shared
        .private_caps
        .instance_flags
        .contains(wgt::InstanceFlags::DEBUG)
        && !device
            .shared
            .private_caps
            .workarounds
            .avoid_shader_debug_info
    {
        compile_args.push(Dxc::DXC_ARG_DEBUG);
        compile_args.push(Dxc::DXC_ARG_SKIP_OPTIMIZATIONS);
    }

    if device.features.contains(wgt::Features::SHADER_F16) {
        compile_args.push(windows::core::w!("-enable-16bit-types"));
    }

    let buffer = Dxc::DxcBuffer {
        Ptr: source.as_ptr().cast(),
        Size: source.len(),
        Encoding: Dxc::DXC_CP_UTF8.0,
    };

    let compile_res: Dxc::IDxcResult =
        unsafe { compiler.Compile(&buffer, Some(&compile_args), None) }
            .into_device_result("Compile")?;

    drop(compile_args);
    drop(source_name);
    drop(raw_ep);
    drop(full_stage);

    let err_blob = get_output::<Dxc::IDxcBlobUtf8>(&compile_res, Dxc::DXC_OUT_ERRORS)?;

    let len = unsafe { err_blob.GetStringLength() };
    if len != 0 {
        let err = as_err_str(&err_blob)?;
        return Err(crate::PipelineError::Linkage(
            stage_bit,
            format!("DXC compile error: {err}"),
        ));
    }

    let blob = get_output::<Dxc::IDxcBlob>(&compile_res, Dxc::DXC_OUT_OBJECT)?;
    if let Some(key) = &cache_key {
        // SAFETY: the blob's own buffer, read while the blob lives.
        let code = unsafe {
            core::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
        };
        super::shader_disk_cache::write(key, code);
    }

    Ok(crate::dx12::CompiledShader::Dxc(blob))
}
