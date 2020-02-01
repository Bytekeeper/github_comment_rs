use base64::encode;
use hyper::Response;
use rand::prelude::*;
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::env;
use std::io::{stdin, stdout, Read, Write};
use std::time::SystemTime;

#[derive(Deserialize, Debug)]
struct Post {
    r#ref: String,
    message: String,
    name: String,
    url: String,
    redirect: String,
}

#[derive(Serialize, Debug)]
struct Comment {
    id: String,
    r#ref: String,
    message: String,
    name: String,
    url: String,
    date: u64,
}

#[derive(Serialize)]
struct CreateRef {
    r#ref: String,
    sha: String,
}

#[derive(Serialize)]
struct CreateFile {
    message: String,
    content: String,
    branch: String,
    committer: UserRef,
}

#[derive(Serialize)]
struct UserRef {
    name: String,
    email: String,
}

#[derive(Serialize)]
struct CreatePR {
    title: String,
    head: String,
    base: String,
}

struct GitHub<'a> {
    client: reqwest::blocking::Client,
    owner: &'a str,
    repo: &'a str,
}

const GITHUB_BASE_URL : &str = "https://api.github.com/repos/";

impl GitHub<'_> {
    fn new<'a>(token: &'a str, owner: &'a str, repo: &'a str) -> GitHub<'a> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("Comment-Bridger"),
        );
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(token).unwrap(),
        );

        let client = reqwest::blocking::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        GitHub {
            owner,
            repo,
            client,
        }
    }

    fn get_branch(&self, branch: &str) -> reqwest::Result<reqwest::blocking::Response> {
        let url = url::Url::parse(
            format!(
                "{}{}/{}/branches/{}",
                GITHUB_BASE_URL, self.owner, self.repo, branch
            )
            .as_str(),
        )
        .unwrap();
        self.client.get(url).send()
    }

    fn create_ref(&self, create_ref: &CreateRef) -> reqwest::Result<reqwest::blocking::Response> {
        let url = url::Url::parse(
            format!(
                "{}{}/{}/git/refs",
                GITHUB_BASE_URL, self.owner, self.repo
            )
            .as_str(),
        )
        .unwrap();
        self.client.post(url).json(create_ref).send()
    }

    fn create_file(
        &self,
        path: &str,
        create_file: &CreateFile,
    ) -> reqwest::Result<reqwest::blocking::Response> {
        let url = url::Url::parse(
            format!(
                "{}{}/{}/contents/{}",
                GITHUB_BASE_URL, self.owner, self.repo, path
            )
            .as_str(),
        )
        .unwrap();
        self.client.put(url).json(create_file).send()
    }

    fn create_pr(&self, create_pr: &CreatePR) -> reqwest::Result<reqwest::blocking::Response> {
        let url = url::Url::parse(
            format!(
                "{}{}/{}/pulls",
                GITHUB_BASE_URL, self.owner, self.repo
            )
            .as_str(),
        )
        .unwrap();
        self.client.post(url).json(create_pr).send()
    }
}

fn main() {
    let token = &env::var("TOKEN").expect("TOKEN missing");
    let owner = &env::var("REPO_OWNER").expect("REPO_OWNER missing");
    let owner_email = &env::var("REPO_OWNER_EMAIL").expect("REPO_OWNER_EMAIL missing");
    let repo = &env::var("REPO").expect("REPO missing");

    let content_length: usize = env::var("CONTENT_LENGTH")
        .ok()
        .and_then(|cl| cl.parse::<usize>().ok())
        .expect("Content length expected");
    assert!(content_length < 100 * 1024, "Comment size limit exceeded");
    let mut body = vec![0; content_length];
    stdin().read_exact(&mut body).expect("Could not read body");

    let post: Post = serde_urlencoded::from_bytes(body.as_slice()).unwrap();

    let github = GitHub::new(token, owner, repo);
    let master = &github
        .get_branch("master")
        .unwrap()
        .json::<HashMap<String, serde_json::Value>>()
        .unwrap()["commit"]["sha"];
    if let serde_json::Value::String(sha) = master {
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut rng = thread_rng();
        let comment_id = format!("{}_{}", time, rng.gen_range(0, 999999999));

        github
            .create_ref(&CreateRef {
                r#ref: format!("refs/heads/comments/{}", comment_id),
                sha: sha.to_string(),
            })
            .unwrap();

        let comment_file = &format!("_data/comments/{}/{}.yaml", post.r#ref, comment_id);
        let branch_name = format!("comments/{}", comment_id);

        let comment = Comment {
            id: comment_id.to_string(),
            r#ref: post.r#ref.to_string(),
            message: post.message.to_string(),
            name: post.name.to_string(),
            url: post.url.to_string(),
            date: time,
        };

        github
            .create_file(
                comment_file,
                &CreateFile {
                    message: "Comment".to_string(),
                    content: encode(&serde_yaml::to_string(&comment).unwrap()),
                    branch: branch_name.to_string(),
                    committer: UserRef {
                        name: owner.to_string(),
                        email: owner_email.to_string(),
                    },
                },
            )
            .unwrap();

        github
            .create_pr(&CreatePR {
                title: format!("Comment {}", &comment_id),
                head: branch_name.to_string(),
                base: "master".to_string(),
            })
            .unwrap();
    } else {
        panic!("Invalid ref!");
    }
    let response = Response::builder()
        .version(hyper::Version::HTTP_11)
        .status(hyper::StatusCode::SEE_OTHER)
        .header("Location", post.redirect)
        .body(())
        .unwrap();
    let buf = response_to_buf(response);
    stdout().write_all(&buf).unwrap();
}

fn response_to_buf(response: Response<()>) -> Vec<u8> {
    let mut output = String::new();
    output.push_str("Status: ");
    output.push_str(response.status().as_str());
    if let Some(reason) = response.status().canonical_reason() {
        output.push_str(" ");
        output.push_str(reason);
    }
    output.push_str("\n");

    {
        let headers = response.headers();
        let mut keys: Vec<&hyper::header::HeaderName> = headers.keys().collect();
        keys.sort_by_key(|h| h.as_str());
        for key in keys {
            output.push_str(key.as_str());
            output.push_str(": ");
            output.push_str(headers.get(key).unwrap().to_str().unwrap());
            output.push_str("\n");
        }
    }

    output.push_str("\n");

    output.into_bytes()
}
