use actix_files::Files;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, post, web};
use chrono::Local;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    process::Command,
    str::FromStr,
    sync::{Arc, LazyLock, Mutex},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
enum Format {
    Video,
    Audio,
}

impl Default for Format {
    fn default() -> Self {
        Self::Video
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct DownloadRequest {
    id: Uuid,
    url: String,
    #[serde(default)]
    format: Format,
}

#[derive(Clone)]
struct DownloadRequestItem {
    id: Uuid,
    url: String,
    session: Uuid,
    format: Format,
}

#[derive(Clone, Serialize)]
enum State {
    Enqueued,
    InProgress,
    Finished,
    Failed,
}

#[derive(Serialize)]
struct DownloadStates {
    jobs: HashMap<Uuid, State>,
}

#[derive(Clone)]
struct States {
    states: Arc<Mutex<HashMap<Uuid, HashMap<Uuid, State>>>>,
}

impl States {
    pub fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_state(&self, session: &Uuid, id: &Uuid, state: &State) -> Result<(), String> {
        let states_lock = self.states.lock();
        if let Err(err) = &states_lock {
            log::warn!("error setting state to download id {}: {}", id, err);
            return Err("error setting state to download".to_string());
        }

        let mut states = states_lock.unwrap();
        let mut session_states = match states.get(session) {
            Some(session_states) => session_states.clone(),
            None => HashMap::new(),
        };

        session_states.insert(id.clone(), state.clone());
        states.insert(session.clone(), session_states.clone());
        Ok(())
    }

    pub fn get_states(&self, session: &Uuid) -> Result<HashMap<Uuid, State>, String> {
        let states_lock = self.states.lock();
        if let Err(err) = &states_lock {
            log::warn!("error retrieving session states: {}", err);
            return Err("error retrieving session states".to_string());
        }

        let states = states_lock.unwrap();
        let session_states = match states.get(session) {
            Some(session_states) => session_states.clone(),
            None => HashMap::new(),
        };

        Ok(session_states)
    }
}

#[derive(Clone)]
struct Queue {
    requests: Arc<Mutex<VecDeque<DownloadRequestItem>>>,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn add(&self, request: &DownloadRequestItem) -> Result<(), String> {
        let requests_lock = self.requests.lock();
        if let Err(err) = &requests_lock {
            log::warn!("error adding request to queue: {}", err);
            return Err("unable to queue download request".to_string());
        }

        let mut requests = requests_lock.unwrap();
        requests.push_back(request.clone());
        Ok(())
    }

    pub fn pull(&self) -> Option<DownloadRequestItem> {
        let requests_lock = self.requests.lock();
        if let Err(err) = &requests_lock {
            log::warn!(
                "error locking requests lock while pulling latest request: {}",
                err
            );
            return None;
        }

        let mut requests = requests_lock.unwrap();
        requests.pop_front()
    }
}

static QUEUE: LazyLock<Queue> = LazyLock::new(|| Queue::new());
static STATES: LazyLock<States> = LazyLock::new(|| States::new());
const DOWNLOADS_DIR: &str = "downloads";
const SESSION_COOKIE: &str = "SESSION";
const WEB_DIST_DIR: &str = "web/dist";

/* fn download_path(id: &Uuid, format: &Format) -> std::io::Result<PathBuf> {
    let exists = std::fs::exists(DOWNLOADS_DIR)?;

    if !exists {
        std::fs::create_dir(DOWNLOADS_DIR)?;
    }

    let filename = format!("{}{}", id.to_string(), format.file_extension());
    let path = Path::new(DOWNLOADS_DIR).join(filename);

    Ok(path)
} */

fn find_download(id: &Uuid) -> Option<PathBuf> {
    let files = std::fs::read_dir(DOWNLOADS_DIR);
    if let Err(err) = &files {
        log::warn!("error reading downloads dir: {}", err);
        return None;
    }

    let files = files.unwrap();
    for file in files {
        if let Err(err) = &file {
            log::warn!("failed reading file entry: {}", err);
            continue;
        }

        let file = file.unwrap();
        let filename = file.file_name();
        let filename = filename.to_string_lossy();
        let path = file.path();

        if filename.starts_with(id.to_string().as_str()) {
            log::debug!("found donwload by id {}: {:?}", id, path);
            return Some(path);
        }
    }

    None
}

fn download(request: &DownloadRequestItem) -> Result<(), String> {
    let url = request.url.as_str();
    let mut command = Command::new("yt-dlp");

    match request.format {
        Format::Video => command.args([
            "-S",
            "res,ext:mp4:m4a",
            "--recode-video",
            "mp4",
            "-o",
            format!("{}/{}.%(ext)s", DOWNLOADS_DIR, request.id).as_str(),
            url,
        ]),
        Format::Audio => command.args([
            "-x",
            "--audio-format",
            "m4a",
            "--embed-thumbnail",
            "--embed-metadata",
            "-o",
            format!("{}/{}.%(ext)s", DOWNLOADS_DIR, request.id).as_str(),
            url,
        ]),
    };

    let process = command.spawn();
    if let Err(err) = &process {
        log::warn!("error launching yt-dlp for: {}: {}", url, err);
        return Err("error launching yt-dlp".to_string());
    }

    let process = process.unwrap();
    let output = process.wait_with_output();
    if let Err(err) = &output {
        log::warn!("failed get yt-dlp output: {}", err);
        return Err("yt-dlp error".to_string());
    }

    let output = output.unwrap();
    let exit_code = output.status.code().unwrap_or(0);

    if !output.status.success() {
        return Err(format!("yt-dlp exited with exit code: {}", exit_code));
    }

    Ok(())
}

fn spawn_download_queue_job() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        log::info!("download worker initialized");

        loop {
            let request = match QUEUE.pull() {
                Some(request) => request,
                None => continue,
            };

            log::info!(
                "processing download request {} - {}",
                &request.id,
                &request.url
            );

            let _ = STATES.set_state(&request.session, &request.id, &State::InProgress);

            match download(&request) {
                Ok(()) => {
                    let _ = STATES.set_state(&request.session, &request.id, &State::Finished);
                    log::info!("success download {} - {}", &request.id, &request.url);
                }
                Err(err) => {
                    let _ = STATES.set_state(&request.session, &request.id, &State::Failed);
                    log::warn!(
                        "error downloading {} - {}: {}",
                        &request.id,
                        &request.url,
                        err
                    )
                }
            };
        }
    })
}

#[post("/api/enqueue")]
async fn post_enqueue(
    download_request: web::Json<DownloadRequest>,
    request: HttpRequest,
) -> impl Responder {
    let session_id = match request.cookie(SESSION_COOKIE) {
        Some(cookie) => &cookie.value().to_string(),
        None => return HttpResponse::BadRequest().finish(),
    };

    let session_id = match Uuid::from_str(session_id.as_str()) {
        Ok(value) => value,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let request_item = DownloadRequestItem {
        id: download_request.id.clone(),
        url: download_request.url.clone(),
        session: session_id.clone(),
        format: download_request.format.clone(),
    };

    if let Err(_) = QUEUE.add(&request_item) {
        return HttpResponse::InternalServerError().finish();
    }

    if let Err(_) = STATES.set_state(&session_id, &download_request.id, &State::Enqueued) {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}

#[get("/api/status")]
async fn get_status(request: HttpRequest) -> impl Responder {
    let session_id = match request.cookie(SESSION_COOKIE) {
        Some(cookie) => &cookie.value().to_string(),
        None => return HttpResponse::BadRequest().finish(),
    };

    let session_id = match Uuid::from_str(session_id.as_str()) {
        Ok(value) => value,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let session_states = STATES.get_states(&session_id);
    if let Err(_) = &session_states {
        return HttpResponse::InternalServerError().finish();
    }

    let response_body = DownloadStates {
        jobs: session_states.unwrap(),
    };

    HttpResponse::Ok().json(response_body)
}

#[get("/api/download/{id}")]
async fn get_download(id: web::Path<String>) -> impl Responder {
    let id = match Uuid::from_str(id.into_inner().as_str()) {
        Ok(uuid) => uuid,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let path = match find_download(&id) {
        Some(path) => path,
        None => return HttpResponse::NotFound().finish(),
    };

    let filename = match path.file_name() {
        Some(filename) => filename.to_string_lossy(),
        None => {
            log::warn!("can't resolve file name from path: {:?}", path);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(err) => {
            log::warn!("error opening file {:?}: {}", path, err);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(err) => {
            log::warn!("error obtaining file metadata {:?}: {}", path, err);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let reader = ReaderStream::new(file);

    HttpResponse::Ok()
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"konakona_{}\"", filename),
        ))
        .insert_header(("Content-Length", metadata.len()))
        .streaming(reader)
}

fn init_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .level(LevelFilter::Debug)
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .chain(std::io::stdout())
        .chain(fern::log_file("konakona.log")?)
        .apply()?;

    Ok(())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    init_logger().expect("failed to initialize logger");
    let _job = spawn_download_queue_job();

    HttpServer::new(|| {
        App::new()
            .service(post_enqueue)
            .service(get_status)
            .service(get_download)
            .service(Files::new("/", WEB_DIST_DIR).index_file("index.html"))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
