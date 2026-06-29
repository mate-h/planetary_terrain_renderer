use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, extract_resource::ExtractResource,
        extract_resource::ExtractResourcePlugin, renderer::RenderDevice,
    },
};
use objc2_foundation::{NSString, NSURL};
use objc2_metal::{MTLCaptureDescriptor, MTLCaptureDestination, MTLCaptureManager};
use std::{env, fs, time::SystemTime};
use wgpu_core::api::Metal;

pub struct MetalCapturePlugin;

impl Plugin for MetalCapturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameCapture>()
            .add_plugins(ExtractResourcePlugin::<FrameCapture>::default())
            .add_systems(Update, input_capture);

        app.sub_app_mut(RenderApp)
            .add_systems(
                Render,
                start_capture.in_set(RenderSystems::PrepareBindGroups),
            )
            .add_systems(Render, stop_capture.in_set(RenderSystems::Cleanup));
    }
}

#[derive(Clone, Default, Resource, ExtractResource)]
pub struct FrameCapture {
    pub(crate) capture: bool,
}

pub fn input_capture(input: Res<ButtonInput<KeyCode>>, mut capture: ResMut<FrameCapture>) {
    capture.capture = input.just_pressed(KeyCode::KeyC);
}

pub fn start_capture(capture: Res<FrameCapture>, device: Res<RenderDevice>) {
    if !capture.capture {
        return;
    }

    println!("Capturing frame");

    let output_path = env::current_dir()
        .expect("current directory")
        .join("captures")
        .join(format!(
            "capture_{}.gputrace",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_secs()
        ));

    if let Some(parent) = output_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!("Failed to create captures directory: {err}");
            return;
        }
    }

    let descriptor = MTLCaptureDescriptor::new();
    descriptor.setDestination(MTLCaptureDestination::GPUTraceDocument);

    let path = NSString::from_str(
        output_path
            .to_str()
            .expect("capture output path must be valid UTF-8"),
    );
    let url = NSURL::fileURLWithPath(&path);
    descriptor.setOutputURL(Some(&url));

    let Some(hal_device) = (unsafe { device.wgpu_device().as_hal::<Metal>() }) else {
        eprintln!("Failed to get Metal HAL device");
        return;
    };
    descriptor.set_capture_device(hal_device.raw_device());

    let manager = unsafe { MTLCaptureManager::sharedCaptureManager() };
    if !manager.supportsDestination(MTLCaptureDestination::GPUTraceDocument) {
        eprintln!(
            "GPU trace capture is not supported (is METAL_CAPTURE_ENABLED set before startup?)"
        );
        return;
    }

    if let Err(err) = manager.startCaptureWithDescriptor_error(&descriptor) {
        eprintln!(
            "Failed to start capture (code {}): {}",
            err.code(),
            err.localizedDescription()
        );
    } else {
        println!("Capture started, writing to {}", output_path.display());
    }
}

pub fn stop_capture(capture: Res<FrameCapture>) {
    if !capture.capture {
        return;
    }

    let manager = unsafe { MTLCaptureManager::sharedCaptureManager() };
    manager.stopCapture();
    println!("Capture stopped");
}
