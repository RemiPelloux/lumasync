use super::frame::ScreenFrame;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, HMODULE},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
};

/// Single-threaded, queue-free duplication. The staging texture and CPU buffer
/// survive frames; no parked recorder thread can hand us yesterday's frame.
pub(super) struct DesktopCapture {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
}

impl DesktopCapture {
    pub(super) fn new(origin: (i32, i32)) -> Result<Self> {
        let (adapter, output) = find_output(origin)?;
        let (device, context) = create_device(&adapter)?;
        // SAFETY: output and device belong to the same enumerated adapter.
        let duplication = unsafe { output.DuplicateOutput(&device)? };
        Ok(Self {
            device,
            context,
            duplication,
            staging: None,
            size: (0, 0),
        })
    }

    pub(super) fn update(&mut self, frame: &mut ScreenFrame) -> Result<bool> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // SAFETY: valid COM handles; out parameters live through the call.
        let acquired = unsafe {
            self.duplication
                .AcquireNextFrame(0, &mut info, &mut resource)
        };
        if let Err(error) = acquired {
            return if error.code() == DXGI_ERROR_WAIT_TIMEOUT {
                Ok(false)
            } else {
                Err(error)
            };
        }
        let result = if info.LastPresentTime == 0 {
            Ok(false)
        } else {
            resource
                .ok_or_else(|| Error::from(E_FAIL))
                .and_then(|resource| self.copy(&resource, frame))
                .map(|()| true)
        };
        // Always release acquired frames, including copy/map failures.
        let released = unsafe { self.duplication.ReleaseFrame() };
        result.and_then(|fresh| released.map(|()| fresh))
    }

    fn copy(&mut self, resource: &IDXGIResource, frame: &mut ScreenFrame) -> Result<()> {
        let texture: ID3D11Texture2D = resource.cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: texture is a live duplication resource.
        unsafe { texture.GetDesc(&mut desc) };
        if desc.Width == 0 || desc.Height == 0 || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(Error::from(E_FAIL));
        }
        self.prepare_staging(desc)?;
        let staging = self.staging.as_ref().ok_or_else(|| Error::from(E_FAIL))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: matching dimensions/format; staging has CPU read access.
        unsafe {
            self.context.CopyResource(staging, &texture);
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        }
        let result = copy_rows(frame, &mapped, self.size);
        // SAFETY: successful Map above, exactly one matching Unmap.
        unsafe { self.context.Unmap(staging, 0) };
        result
    }

    fn prepare_staging(&mut self, mut desc: D3D11_TEXTURE2D_DESC) -> Result<()> {
        if self.size == (desc.Width, desc.Height) && self.staging.is_some() {
            return Ok(());
        }
        self.staging = None;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        // SAFETY: source descriptor adapted to a read-only staging texture.
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut self.staging))?
        };
        self.size = (desc.Width, desc.Height);
        Ok(())
    }
}

fn find_output(origin: (i32, i32)) -> Result<(IDXGIAdapter1, IDXGIOutput1)> {
    // SAFETY: COM enumeration owns the returned interface references.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            if let Some(output) = adapter_output(&adapter, origin)? {
                return Ok((adapter, output));
            }
        }
    }
    Err(Error::from(E_FAIL))
}

fn adapter_output(adapter: &IDXGIAdapter1, origin: (i32, i32)) -> Result<Option<IDXGIOutput1>> {
    let mut index = 0;
    // SAFETY: live adapter; each enumerated output owns its reference.
    unsafe {
        while let Ok(output) = adapter.EnumOutputs(index) {
            index += 1;
            let desc = output.GetDesc()?;
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            if (desc.DesktopCoordinates.left, desc.DesktopCoordinates.top) != origin {
                continue;
            }
            // Rotated output needs coordinate rotation: use the existing fallback.
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                return Err(Error::from(E_FAIL));
            }
            return Ok(Some(output.cast()?));
        }
    }
    Ok(None)
}

fn create_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: explicit adapter requires UNKNOWN driver type; valid out pointers.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((
        device.ok_or_else(|| Error::from(E_FAIL))?,
        context.ok_or_else(|| Error::from(E_FAIL))?,
    ))
}

fn copy_rows(
    frame: &mut ScreenFrame,
    mapped: &D3D11_MAPPED_SUBRESOURCE,
    size: (u32, u32),
) -> Result<()> {
    let row_bytes = (size.0 as usize)
        .checked_mul(4)
        .ok_or_else(|| Error::from(E_FAIL))?;
    let length = row_bytes
        .checked_mul(size.1 as usize)
        .ok_or_else(|| Error::from(E_FAIL))?;
    if mapped.pData.is_null() || (mapped.RowPitch as usize) < row_bytes {
        return Err(Error::from(E_FAIL));
    }
    frame.pixels.resize(length, 0);
    for (row, destination) in frame.pixels.chunks_exact_mut(row_bytes).enumerate() {
        // SAFETY: Map exposes Height rows of RowPitch bytes until Unmap;
        // the destination is disjoint owned storage and only Width pixels copy.
        unsafe {
            let source = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
            destination.copy_from_slice(std::slice::from_raw_parts(source, row_bytes));
        }
    }
    frame.width = size.0;
    frame.height = size.1;
    frame.bgra = true;
    Ok(())
}
