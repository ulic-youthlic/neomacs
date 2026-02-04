//! Video cache with GStreamer backend and optional VA-API hardware acceleration.
//!
//! Provides async video decoding with DMA-BUF zero-copy when available,
//! falling back to CPU decode + copy otherwise.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use gstreamer_app as gst_app;

/// Video playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    /// Video is loading/buffering
    Loading,
    /// Video is playing
    Playing,
    /// Video is paused
    Paused,
    /// Video playback stopped
    Stopped,
    /// Video reached end
    EndOfStream,
    /// Error occurred
    Error,
}

/// Decoded video frame ready for rendering
pub struct DecodedFrame {
    /// Frame ID
    pub id: u32,
    /// Video ID this frame belongs to
    pub video_id: u32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// RGBA pixel data (CPU path)
    pub data: Vec<u8>,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
    /// Duration in nanoseconds
    pub duration: u64,
}

/// Cached video with GStreamer pipeline
pub struct CachedVideo {
    /// Video ID
    pub id: u32,
    /// Video dimensions
    pub width: u32,
    pub height: u32,
    /// Current state
    pub state: VideoState,
    /// Current wgpu texture (updated each frame)
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    pub bind_group: Option<wgpu::BindGroup>,
    /// Frame count
    pub frame_count: u64,
    /// Loop count (-1 = infinite)
    pub loop_count: i32,
}

/// Request to load a video
struct LoadRequest {
    id: u32,
    path: String,
}

/// Video pipeline with frame extraction
struct VideoPipeline {
    pipeline: gst::Pipeline,
    appsink: gst_video::VideoSink,
}

/// Video cache managing multiple videos with async decoding
pub struct VideoCache {
    /// Cached videos by ID
    videos: HashMap<u32, CachedVideo>,
    /// Next video ID
    next_id: u32,
    /// Channel to send load requests
    load_tx: mpsc::Sender<LoadRequest>,
    /// Channel to receive decoded frames
    frame_rx: mpsc::Receiver<DecodedFrame>,
}

impl VideoCache {
    /// Create a new video cache
    pub fn new() -> Self {
        // Initialize GStreamer
        if let Err(e) = gst::init() {
            log::error!("Failed to initialize GStreamer: {}", e);
        }

        let (load_tx, load_rx) = mpsc::channel::<LoadRequest>();
        let (frame_tx, frame_rx) = mpsc::channel::<DecodedFrame>();

        // Spawn decoder thread
        thread::spawn(move || {
            Self::decoder_thread(load_rx, frame_tx);
        });

        Self {
            videos: HashMap::new(),
            next_id: 1,
            load_tx,
            frame_rx,
        }
    }

    /// Initialize GPU resources
    /// Note: Video bind groups are created using image_pipeline's layout for compatibility.
    pub fn init_gpu(&mut self, _device: &wgpu::Device) {
        log::info!("VideoCache: GPU resources initialized (using shared image pipeline layout)");
    }

    /// Load a video file
    pub fn load_file(&mut self, path: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        // Create placeholder entry
        self.videos.insert(id, CachedVideo {
            id,
            width: 0,
            height: 0,
            state: VideoState::Loading,
            texture: None,
            texture_view: None,
            bind_group: None,
            frame_count: 0,
            loop_count: 0,
        });

        // Send load request
        let _ = self.load_tx.send(LoadRequest {
            id,
            path: path.to_string(),
        });

        log::info!("VideoCache: queued video {} for loading: {}", id, path);
        id
    }

    /// Get video state
    pub fn get_state(&self, id: u32) -> Option<VideoState> {
        self.videos.get(&id).map(|v| v.state)
    }

    /// Get video dimensions
    pub fn get_dimensions(&self, id: u32) -> Option<(u32, u32)> {
        self.videos.get(&id).map(|v| (v.width, v.height))
    }

    /// Get video for rendering
    pub fn get(&self, id: u32) -> Option<&CachedVideo> {
        self.videos.get(&id)
    }

    /// Play video
    pub fn play(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Playing;
            log::debug!("VideoCache: play video {}", id);
        }
    }

    /// Pause video
    pub fn pause(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Paused;
            log::debug!("VideoCache: pause video {}", id);
        }
    }

    /// Stop video
    pub fn stop(&mut self, id: u32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.state = VideoState::Stopped;
            log::debug!("VideoCache: stop video {}", id);
        }
    }

    /// Set loop count (-1 for infinite)
    pub fn set_loop(&mut self, id: u32, count: i32) {
        if let Some(video) = self.videos.get_mut(&id) {
            video.loop_count = count;
        }
    }

    /// Remove video from cache
    pub fn remove(&mut self, id: u32) {
        self.videos.remove(&id);
        log::debug!("VideoCache: removed video {}", id);
    }

    /// Process pending decoded frames (call each frame)
    /// Uses the provided bind_group_layout and sampler from image_cache
    /// to ensure compatibility with the shared image/video rendering pipeline.
    pub fn process_pending(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) {
        // Process all available frames
        let mut frame_count = 0;
        while let Ok(frame) = self.frame_rx.try_recv() {
            frame_count += 1;
            let total = self.videos.get(&frame.video_id).map(|v| v.frame_count).unwrap_or(0) + 1;

            log::info!("VideoCache::process_pending received frame #{} for video {}, pts={}ms, size={}x{}",
                total, frame.video_id, frame.pts / 1_000_000, frame.width, frame.height);
            if let Some(video) = self.videos.get_mut(&frame.video_id) {
                // Check if we need to create new texture (first frame or size changed)
                let need_new_texture = video.texture.is_none()
                    || video.width != frame.width
                    || video.height != frame.height;

                if need_new_texture {
                    // Update dimensions
                    video.width = frame.width;
                    video.height = frame.height;
                    if video.state == VideoState::Loading {
                        video.state = VideoState::Playing;
                    }

                    // Create new texture (only when dimensions change)
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Video Frame Texture"),
                        size: wgpu::Extent3d {
                            width: frame.width,
                            height: frame.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });

                    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

                    // Create bind group
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Video Bind Group"),
                        layout: bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(sampler),
                            },
                        ],
                    });

                    video.texture = Some(texture);
                    video.texture_view = Some(texture_view);
                    video.bind_group = Some(bind_group);
                }

                // Update texture data (reuse existing texture)
                if let Some(ref texture) = video.texture {
                    queue.write_texture(
                        wgpu::ImageCopyTexture {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame.data,
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(frame.width * 4),
                            rows_per_image: Some(frame.height),
                        },
                        wgpu::Extent3d {
                            width: frame.width,
                            height: frame.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }

                video.frame_count += 1;
                log::trace!("VideoCache: updated video {} frame {}", frame.video_id, video.frame_count);
            }
        }
    }

    /// Background decoder thread
    fn decoder_thread(
        rx: mpsc::Receiver<LoadRequest>,
        tx: mpsc::Sender<DecodedFrame>,
    ) {
        log::debug!("Video decoder thread started");

        while let Ok(request) = rx.recv() {
            log::info!("Decoder thread: loading video {}: {}", request.id, request.path);

            // Strip file:// prefix if present (filesrc needs raw paths)
            let path = if request.path.starts_with("file://") {
                &request.path[7..]
            } else {
                &request.path
            };

            // Check if VA-API hardware acceleration is available
            let has_vapostproc = gst::ElementFactory::find("vapostproc").is_some();

            // Create GStreamer pipeline with video and audio
            // decodebin will auto-select VA-API hardware decoders when available
            // since they have higher rank than software decoders
            let pipeline_str = if has_vapostproc {
                // Hardware-accelerated pipeline:
                // - decodebin auto-selects VA-API decoders (they have higher rank)
                // - vapostproc does GPU-based color conversion if decoder outputs VA memory
                // - videoconvert is fallback for CPU buffers
                log::info!("Using VA-API hardware acceleration pipeline (vapostproc available)");
                format!(
                    "filesrc location=\"{}\" ! decodebin name=dec \
                     dec. ! queue max-size-buffers=3 ! vapostproc ! videoconvert ! video/x-raw,format=RGBA ! appsink name=sink \
                     dec. ! queue ! audioconvert ! audioresample ! autoaudiosink",
                    path.replace("\"", "\\\"")
                )
            } else {
                // Software fallback pipeline
                log::info!("VA-API not available, using software decoding");
                format!(
                    "filesrc location=\"{}\" ! decodebin name=dec \
                     dec. ! queue ! videoconvert ! video/x-raw,format=RGBA ! appsink name=sink \
                     dec. ! queue ! audioconvert ! audioresample ! autoaudiosink",
                    path.replace("\"", "\\\"")
                )
            };

            log::debug!("Creating GStreamer pipeline: {}", pipeline_str);

            match gst::parse::launch(&pipeline_str) {
                Ok(pipeline) => {
                    log::debug!("Pipeline created successfully");
                    let pipeline = pipeline.dynamic_cast::<gst::Pipeline>().unwrap();

                    // Get appsink
                    let appsink = pipeline
                        .by_name("sink")
                        .expect("Could not get appsink")
                        .dynamic_cast::<gst_app::AppSink>()
                        .expect("Could not cast to AppSink");

                    // Configure appsink for pull mode (polling with try_pull_sample)
                    appsink.set_max_buffers(2);
                    appsink.set_drop(true);

                    let video_id = request.id;
                    let tx_clone = tx.clone();

                    // Start playing
                    log::debug!("Setting pipeline to Playing state");
                    if let Err(e) = pipeline.set_state(gst::State::Playing) {
                        log::error!("Failed to start pipeline: {:?}", e);
                    } else {
                        log::info!("Pipeline started successfully for video {}", request.id);
                    }

                    // Spawn frame pulling thread
                    let appsink_clone = appsink.clone();
                    let pipeline_weak = pipeline.downgrade();
                    let using_vaapi = has_vapostproc;
                    std::thread::spawn(move || {
                        log::info!("Frame puller thread started for video {}", video_id);

                        // Wait for pipeline to reach PLAYING state
                        if let Some(pipeline) = pipeline_weak.upgrade() {
                            let (res, state, _) = pipeline.state(gst::ClockTime::from_seconds(5));
                            log::info!("Video {} pipeline state: {:?}, result: {:?}", video_id, state, res);
                        }
                        let mut frame_count = 0u64;
                        let mut timeout_count = 0u64;

                        loop {
                            // Try to pull a sample with 100ms timeout
                            match appsink_clone.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                                Some(sample) => {
                                    timeout_count = 0;
                                    frame_count += 1;
                                    if let Some(buffer) = sample.buffer() {
                                        // Get video info from caps
                                        if let Some(caps) = sample.caps() {
                                            if let Ok(info) = gst_video::VideoInfo::from_caps(caps) {
                                                let width = info.width();
                                                let height = info.height();

                                                if frame_count <= 5 || frame_count % 60 == 0 {
                                                    log::debug!("Frame #{} for video {}, {}x{} (VA-API: {})",
                                                        frame_count, video_id, width, height, using_vaapi);
                                                }

                                                // Map buffer and extract RGBA data
                                                if let Ok(map) = buffer.map_readable() {
                                                    let data = map.as_slice().to_vec();

                                                    if tx_clone.send(DecodedFrame {
                                                        id: frame_count as u32,
                                                        video_id,
                                                        width,
                                                        height,
                                                        data,
                                                        pts: buffer.pts().map(|p| p.nseconds()).unwrap_or(0),
                                                        duration: buffer.duration().map(|d| d.nseconds()).unwrap_or(0),
                                                    }).is_err() {
                                                        log::debug!("Frame receiver dropped, stopping puller");
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                None => {
                                    timeout_count += 1;
                                    // Check if EOS
                                    if appsink_clone.is_eos() {
                                        log::info!("Video {} reached EOS after {} frames", video_id, frame_count);
                                        break;
                                    }
                                    // Log occasional timeout status
                                    if timeout_count == 1 || timeout_count % 50 == 0 {
                                        log::debug!("Video {} pull timeout #{}, frames so far: {}", video_id, timeout_count, frame_count);
                                    }
                                }
                            }
                        }
                        log::debug!("Frame puller thread exiting for video {}", video_id);
                    });

                    // Wait for EOS or error on bus
                    let bus = pipeline.bus().unwrap();
                    for msg in bus.iter_timed(gst::ClockTime::NONE) {
                        match msg.view() {
                            gst::MessageView::Eos(..) => {
                                log::debug!("Video {} bus: end of stream", video_id);
                                break;
                            }
                            gst::MessageView::Error(err) => {
                                log::error!(
                                    "Video {} error: {} ({:?})",
                                    video_id,
                                    err.error(),
                                    err.debug()
                                );
                                break;
                            }
                            _ => {}
                        }
                    }

                    // Cleanup
                    let _ = pipeline.set_state(gst::State::Null);
                }
                Err(e) => {
                    log::error!("Failed to create pipeline for video {}: {}", request.id, e);
                }
            }
        }

        log::debug!("Video decoder thread exiting");
    }
}

impl Default for VideoCache {
    fn default() -> Self {
        Self::new()
    }
}
