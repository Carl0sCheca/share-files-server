use std::path::Path;

use actix_web::{
    get, middleware, post,
    web::{self, Bytes},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};

use chrono::{DateTime, Local};
use minio_rsc::{client::Tags, provider::StaticProvider, Minio};

const BUCKET_NAME: &str = "share-files";

#[derive(serde::Serialize)]
enum UploadResponse {
    Ok { message: String },
    Error { message: String },
}

fn minio_provider() -> StaticProvider {
    let minio_username = std::env::var("MINIO_ROOT_USER")
        .ok()
        .unwrap_or("miniousername".to_owned());

    let minio_password = std::env::var("MINIO_ROOT_PASSWORD")
        .ok()
        .unwrap_or("miniopassword".to_owned());

    StaticProvider::new(minio_username, minio_password, None)
}

#[post("/upload")]
async fn upload(req: HttpRequest, payload: actix_web::web::Bytes) -> impl Responder {
    let secret_token = std::env::var("SECRET_TOKEN").unwrap();

    let filename = req
        .headers()
        .clone()
        .get("share-filename")
        .map(|value| String::from_utf8(value.as_bytes().to_vec()).unwrap());

    let format = {
        if let Some(file_name) = filename.clone() {
            let parts: Vec<&str> = file_name.split('.').collect();

            if !parts.is_empty() {
                if let Some(extension) = parts.last() {
                    match *extension {
                        ">sc" => "png".to_owned(),
                        ">txt" => "txt".to_owned(),
                        value => match parts.len() {
                            2.. => value.to_owned(),
                            _ => "".to_owned(),
                        },
                    }
                } else {
                    "txt".to_owned()
                }
            } else {
                "txt".to_owned()
            }
        } else {
            "txt".to_owned()
        }
    };

    let filename = match filename {
        Some(name) => match name.as_str() {
            ">sc" => None,
            ">txt" => None,
            value => Some(value.to_owned()),
        },
        None => None,
    };

    match req.headers().clone().get("share-token") {
        Some(value) if value.eq(secret_token.as_str()) => (),
        _ => {
            return web::Json(UploadResponse::Error {
                message: "Invalid token".to_owned(),
            });
        }
    };

    let filename_hash = {
        let now = Local::now();
        let timestamp_str = now.to_string();
        let mut hasher = sha1_smol::Sha1::new();
        hasher.update(timestamp_str.as_bytes());
        let result: String = hasher.digest().to_string();

        let short_result = result[0..10].to_owned();

        match format {
            x if x.is_empty() => short_result,
            format => format!("{short_result}.{format}"),
        }
    };

    let provider = minio_provider();

    let minio_endpoint = std::env::var("MINIO_ENDPOINT")
        .ok()
        .unwrap_or("localhost".to_owned());
    let minio_endpoint_port = std::env::var("MINIO_ENDPOINT_PORT")
        .ok()
        .unwrap_or("9000".to_owned());

    let minio = Minio::builder()
        .endpoint(format!("{minio_endpoint}:{minio_endpoint_port}"))
        .provider(provider)
        .secure(false)
        .build()
        .unwrap();

    let (buckets, _owner) = minio.list_buckets().await.unwrap();

    if buckets.is_empty() {
        minio.make_bucket(BUCKET_NAME, false).await.unwrap();
    }

    minio
        .put_object(BUCKET_NAME, filename_hash.to_string(), payload)
        .await
        .unwrap();

    if let Some(filename) = filename {
        let mut tags: Tags = Tags::new();
        tags.insert("filename", filename);
        minio
            .set_object_tags(BUCKET_NAME, filename_hash.to_string(), tags)
            .await
            .unwrap();
    }

    let conn_info = req.connection_info();
    let url = format!("{}://{}", conn_info.scheme(), conn_info.host());

    println!(
        "{}",
        info(format!("A new file has been received: {filename_hash}").as_str())
    );

    web::Json(UploadResponse::Ok {
        message: format!("{url}/{filename_hash}"),
    })
}

#[get("/{file_id}")]
async fn get_file(path: web::Path<String>) -> impl Responder {
    let file_name = path.into_inner();

    let extension = if let Some(value) = Path::new(&file_name).extension() {
        value.to_str().unwrap()
    } else {
        ""
    };

    let mime_type = match extension {
        // common image types
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        // common audio types
        "aac" => "audio/aac",
        "mid" | "midi" => "audio/midi",
        "oga" | "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "weba" => "audio/webm",
        // common video types
        "mp4" => "video/mp4",
        "mpeg" => "video/mpeg",
        "ogv" => "video/ogg",
        "webm" => "video/webm",
        // common text types
        "css" => "text/css",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "js" | "mjs" => "text/javascript",
        "txt" | "" => "text/plain",
        // common application types
        "json" => "application/json",
        "pdf" => "application/pdf",
        // other types or unknown types
        _ => "application/octet-stream",
    };

    let minio_endpoint = std::env::var("MINIO_ENDPOINT")
        .ok()
        .unwrap_or("localhost".to_owned());
    let minio_endpoint_port = std::env::var("MINIO_ENDPOINT_PORT")
        .ok()
        .unwrap_or("9000".to_owned());

    let provider = minio_provider();
    let minio = Minio::builder()
        .endpoint(format!("{minio_endpoint}:{minio_endpoint_port}"))
        .provider(provider)
        .secure(false)
        .build()
        .unwrap();

    if let Ok(value) = minio.get_object(BUCKET_NAME, file_name.clone()).await {
        let tags: Tags = minio.get_object_tags(BUCKET_NAME, file_name).await.unwrap();

        if tags.contains_key("filename") {
            let filename = tags["filename"].clone();
            HttpResponse::Ok()
                .append_header((
                    "Content-Disposition",
                    format!("inline; filename=\"{}\"", filename),
                ))
                .content_type(mime_type)
                .body(value.bytes().await.unwrap())
        } else {
            HttpResponse::Ok()
                .content_type(mime_type)
                .body(value.bytes().await.unwrap())
        }
    } else {
        HttpResponse::NotFound()
            .content_type("text/plain")
            .body("File not found")
    }
}

fn info(message: &str) -> String {
    let now: DateTime<Local> = Local::now();
    format!(
        "{} {}",
        now.format("[%Y-%m-%d %H:%M:%S \x1b[0;32mINFO\x1b[0m share_server]"),
        message
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let secret_token = std::env::var("SECRET_TOKEN");
    if secret_token.is_err() {
        eprintln!("There is no env SECRET_TOKEN");
        std::process::exit(-1);
    }

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(9500);

    let max_payload = std::env::var("max_payload")
        .ok()
        .and_then(|p| p.parse::<usize>().ok())
        .unwrap_or(100);

    println!("{}", info("Starting Server"));
    HttpServer::new(move || {
        App::new()
            .app_data(web::PayloadConfig::new(max_payload * 1024 * 1024))
            .wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::Trim,
            ))
            .default_service(web::route().to(HttpResponse::NotFound))
            .service(
                web::resource("/favicon.ico").route(web::route().to(|| async {
                    let bytes = include_bytes!("../favicon.ico");
                    let data = Bytes::copy_from_slice(bytes);
                    Ok::<_, actix_web::Error>(
                        HttpResponse::Ok().content_type("image/x-icon").body(data),
                    )
                })),
            )
            .service(upload)
            .service(get_file)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}
