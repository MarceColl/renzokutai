use git2::{Repository, TreeWalkMode, TreeWalkResult};
use askama::Template;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::str::FromStr;
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
    link: String,
    filemode: i32,
}

#[derive(Template)]
#[template(path="index.html")]
struct RepoTemplate {
    path: PathBuf,
    repo_name: String,
    commit: RCommit,
    view: RepoView
}

enum RepoView {
    TreeView { files: Vec<RNode>, readme: Option<String> },
    BlobView { content: String },
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

    let (obj, path) = match path {
        Some(axum::extract::Path(path)) => (tree.get_path(Path::new(&path)).unwrap().to_object(&repo).unwrap(), PathBuf::from_str(&path).unwrap()),
        None => (tree.into_object(), PathBuf::from_str("").unwrap()),
    };

    let template = match obj.kind() {
        Some(git2::ObjectType::Blob) => {
            let blob = repo.find_blob(obj.id()).unwrap();
            let content = String::from_utf8(blob.content().to_vec()).unwrap();

            RepoTemplate {
                path: path,
                repo_name,
                commit: RCommit::from_commit(&commit),
                view: RepoView::BlobView {
                    content: content,
                }
            }
        },
        Some(git2::ObjectType::Tree) => {
            let tree = obj.into_tree().unwrap();
            let mut files = Vec::new();

            tree.walk(TreeWalkMode::PreOrder, |root, entry| {
                let name = entry.name().unwrap_or("").to_string();
                let mut obj_path = path.clone();
                obj_path.push(&entry.name().unwrap_or(""));

                println!("{}", obj_path.display());

                if entry.kind() == Some(git2::ObjectType::Blob) {
                    files.push(RNode {
                        link: format!("/repos/{}/{}", repo_name, obj_path.display()),
                        name,
                        kind: entry.kind().unwrap(),
                        filemode: entry.filemode(),
                    });
                    TreeWalkResult::Ok
                } else {
                    files.push(RNode {
                        link: format!("/repos/{}/{}", repo_name, obj_path.display()),
                        name,
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

            RepoTemplate {
                path: path,
                repo_name,
                commit: RCommit::from_commit(&commit),
                view: RepoView::TreeView {
                    files,
                    readme: None,
                }
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
        .route("/repos/renzokutai", get(view_repo))
        .route("/repos/renzokutai/", get(view_repo))
        .route("/repos/renzokutai/{*path}", get(view_repo))
        .nest_service("/static", ServeDir::new("static"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
