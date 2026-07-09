use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::media::ffmpeg::{self, DecodedFrame};

pub(crate) struct PreviewWorker {
    pub(crate) requests: Sender<PreviewRequest>,
    pub(crate) responses: Receiver<PreviewMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviewRequest {
    pub(crate) path: PathBuf,
    pub(crate) anchor_frame: i64,
    pub(crate) anchor_pts: i64,
    pub(crate) start_frame: i64,
    pub(crate) frame_count: i32,
    pub(crate) kind: PreviewRequestKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewRequestKind {
    Interactive,
    Prefetch,
}

pub(crate) enum PreviewMessage {
    Ready {
        accel: String,
    },
    Done {
        request: PreviewRequest,
        result: Result<Vec<DecodedFrame>, String>,
        elapsed: Duration,
    },
    Failed(String),
}

pub(crate) fn spawn_preview_worker(path: PathBuf) -> PreviewWorker {
    let (request_sender, request_receiver) = mpsc::channel::<PreviewRequest>();
    let (response_sender, response_receiver) = mpsc::channel::<PreviewMessage>();
    thread::spawn(move || {
        let mut session = match ffmpeg::open_preview_session(&path) {
            Ok(session) => session,
            Err(error) => {
                let _ = response_sender.send(PreviewMessage::Failed(error));
                return;
            }
        };
        let _ = response_sender.send(PreviewMessage::Ready {
            accel: session.accel_name(),
        });

        while let Ok(mut request) = request_receiver.recv() {
            while let Ok(next) = request_receiver.try_recv() {
                request = next;
            }

            let started_at = Instant::now();
            let result = session.decode_window(
                request.anchor_frame,
                request.anchor_pts,
                request.start_frame,
                request.frame_count,
            );
            let _ = response_sender.send(PreviewMessage::Done {
                request,
                result,
                elapsed: started_at.elapsed(),
            });
        }
    });

    PreviewWorker {
        requests: request_sender,
        responses: response_receiver,
    }
}
