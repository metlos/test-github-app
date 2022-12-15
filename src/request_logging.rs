use std::{fmt, sync::Arc};

use axum::{
    body::Bytes,
    extract::State,
    http::{request, response},
    middleware::Next,
    response::{IntoResponse, Response},
};
use hyper::{Body, Request, StatusCode};
use tokio::io::AsyncWriteExt;
use tokio::{fs::File, io, sync::Mutex};

pub async fn state(file_name: &str) -> io::Result<Arc<Mutex<File>>> {
    let f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_name)
        .await?;

    Ok(Arc::new(Mutex::new(f)))
}

pub async fn log(
    State(f): State<Arc<Mutex<File>>>,
    req: Request<Body>,
    next: Next<Body>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (parts, body) = req.into_parts();
    let bytes = print_request(f.clone(), &parts, body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let res = next.run(req).await;

    let (parts, body) = res.into_parts();
    let bytes = print_response(f, &parts, body).await?;
    let res = Response::from_parts(parts, Body::from(bytes));

    Ok(res)
}

async fn extract_bytes<B>(body: B) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    match hyper::body::to_bytes(body).await {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read request body: {}", err),
            ));
        }
    }
}

fn internal_error<E: fmt::Display>(err: E) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("failed to unlock file to write: {}", err),
    )
}

async fn print_request<B>(
    f: Arc<Mutex<File>>,
    parts: &request::Parts,
    body: B,
) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = extract_bytes(body).await?;

    let mut wrt = f.lock().await;

    wrt.write(
        format!(
            "> {} {} {:?}\n",
            parts.method.as_str(),
            parts.uri.to_string(),
            parts.version
        )
        .as_bytes(),
    )
    .await
    .map_err(internal_error)?;

    for (k, v) in parts.headers.iter() {
        wrt.write("> ".as_bytes()).await.map_err(internal_error)?;
        wrt.write(k.as_str().as_bytes())
            .await
            .map_err(internal_error)?;
        wrt.write(": ".as_bytes()).await.map_err(internal_error)?;
        wrt.write(v.as_bytes()).await.map_err(internal_error)?;
        wrt.write("\n".as_bytes()).await.map_err(internal_error)?;
    }

    wrt.write(">\n>\n".as_bytes())
        .await
        .map_err(internal_error)?;

    wrt.write(&bytes).await.map_err(internal_error)?;

    wrt.write("\n> --------------------------\n\n".as_bytes())
        .await
        .map_err(internal_error)?;

    Ok(bytes)
}

async fn print_response<B>(
    f: Arc<Mutex<File>>,
    parts: &response::Parts,
    body: B,
) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = extract_bytes(body).await?;

    let mut wrt = f.lock().await;

    wrt.write(format!("< {:?} {}\n", parts.version, parts.status,).as_bytes())
        .await
        .map_err(internal_error)?;

    for (k, v) in parts.headers.iter() {
        wrt.write("< ".as_bytes()).await.map_err(internal_error)?;
        wrt.write(k.as_str().as_bytes())
            .await
            .map_err(internal_error)?;
        wrt.write(": ".as_bytes()).await.map_err(internal_error)?;
        wrt.write(v.as_bytes()).await.map_err(internal_error)?;
        wrt.write("\n".as_bytes()).await.map_err(internal_error)?;
    }

    wrt.write("<\n<\n".as_bytes())
        .await
        .map_err(internal_error)?;

    wrt.write(&bytes).await.map_err(internal_error)?;

    wrt.write("\n< --------------------------\n\n".as_bytes())
        .await
        .map_err(internal_error)?;

    Ok(bytes)
}
