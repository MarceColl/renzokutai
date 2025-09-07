use git2::{Repository, TreeWalkMode, TreeWalkResult};
use askama::Template;
use anyhow::Result;
use std::path::Path;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use tower_http::services::ServeDir;

struct RCommit {
    id: String,
    message: String,
    author: String,
}

impl RCommit {
    pub fn from_commit(c: &git2::Commit) -> Self {
        let author = c.author();

        Self {
            id: format!("{}", c.id()),
            message: c.message().unwrap().to_string(),
            author: format!("{} <{}>", author.name().unwrap(), author.email().unwrap()),
        }
    }
}

struct RNode {
    name: String,
    kind: git2::ObjectType,
    filemode: i32,
}

#[derive(Template)]
#[template(path="index.html")]
enum RepoTemplate {
    TreeView { repo_name: String, commit: RCommit, files: Vec<RNode>, readme: Option<String> },
    BlobView { repo_name: String, commit: RCommit, content: String },
}

pub struct Repo {
    name: String
}

async fn view_repo(
    path: Option<axum::extract::Path<String>>,
    ) -> impl IntoResponse {
    let repo_name = "renzokutai".to_string();
    let repo = Repository::open(".").unwrap();

    let main_ref = repo.find_reference("refs/heads/main")
        .or_else(|_| repo.find_reference("refs/heads/master")).unwrap(); // Fallback to master if main doesn't exist

    let commit = main_ref.peel_to_commit().unwrap();
    let tree = commit.tree().unwrap();

    let entry = match path {
        Some(axum::extract::Path(path)) => tree.get_path(Path::new(&path)).unwrap(),
        None => tree.get_path(Path::new("/")).unwrap(),
    };

    let template = match entry.kind() {
        Some(git2::ObjectType::Blob) => {
            let blob = repo.find_blob(entry.id()).unwrap();
            let content = String::from_utf8(blob.content().to_vec()).unwrap();

            RepoTemplate::BlobView {
                repo_name,
                commit: RCommit::from_commit(&commit),
                content: content,
            }
        },
        Some(git2::ObjectType::Tree) => {
            let tree = entry.to_object(&repo).unwrap().into_tree().unwrap();
            let mut files = Vec::new();

            tree.walk(TreeWalkMode::PreOrder, |root, entry| {
                let file_path = if root.is_empty() {
                    entry.name().unwrap_or("").to_string()
                } else {
                    format!("{}{}", root, entry.name().unwrap_or(""))
                };

                if entry.kind() == Some(git2::ObjectType::Blob) {
                    files.push(RNode {
                        name: file_path,
                        kind: entry.kind().unwrap(),
                        filemode: entry.filemode(),
                    });
                    TreeWalkResult::Ok
                } else {
                    files.push(RNode {
                        name: file_path,
                        kind: entry.kind().unwrap(),
                        filemode: entry.filemode(),
                    });
                    TreeWalkResult::Skip
                }
            }).unwrap();

            /*
            let entry = tree.get_path(Path::new("README.md")).unwrap();
            let blob = repo.find_blob(entry.id()).unwrap();
            let content = String::from_utf8(blob.content().to_vec()).unwrap();
            */

            RepoTemplate::TreeView {
                repo_name: repo_name,
                commit: RCommit::from_commit(&commit),
                files: files,
                readme: None,
            }
        },
        _ => todo!(),
    };

    Html(template.render().unwrap())
}

#[tokio::main]
async fn main() -> Result<()> {

    /*
    let entry = tree.get_path(Path::new("./src/bin/server.rs"))?;
    let blob = repo.find_blob(entry.id())?;
    let content = String::from_utf8(blob.content().to_vec())?;

    println!("{}", content);
    */

    let app = Router::new()
        .route("/repo/renzokutai/{*path}", get(view_repo))
        .nest_service("/static", ServeDir::new("static"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
