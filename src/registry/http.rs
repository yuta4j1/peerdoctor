use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use ureq::Agent;

use super::{Packument, Registry, RegistryError};

const ACCEPT: &str = "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*";
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PACKUMENT_SIZE: u64 = 64 * 1024 * 1024;

pub struct HttpRegistry {
    base: String,
    agent: Agent,
    // 同じパッケージを何度も取得しないよう、実行中は取得した結果を覚えておく
    cache: RefCell<HashMap<String, Packument>>,
}

impl HttpRegistry {
    pub fn new(base: &str) -> Self {
        Self::with_timeout(base, TIMEOUT)
    }

    fn with_timeout(base: &str, timeout: Duration) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            agent: Agent::config_builder()
                .timeout_global(Some(timeout))
                .build()
                .into(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn fetch(&self, name: &str) -> Result<String, RegistryError> {
        let url = format!("{}/{}", self.base, name.replace('/', "%2f"));
        let mut response = self
            .agent
            .get(&url)
            .header("Accept", ACCEPT)
            .call()
            .map_err(|error| classify(name, error))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_PACKUMENT_SIZE)
            .read_to_string()
            .map_err(|error| RegistryError::InvalidResponse {
                name: name.to_string(),
                reason: error.to_string(),
            })
    }
}

impl Registry for HttpRegistry {
    fn packument(&self, name: &str) -> Result<Packument, RegistryError> {
        if let Some(cached) = self.cache.borrow().get(name) {
            return Ok(cached.clone());
        }
        let packument = Packument::parse(name, &self.fetch(name)?)?;
        self.cache
            .borrow_mut()
            .insert(name.to_string(), packument.clone());
        Ok(packument)
    }
}

fn classify(name: &str, error: ureq::Error) -> RegistryError {
    let name = name.to_string();
    match error {
        ureq::Error::StatusCode(404) => RegistryError::NotFound { name },
        ureq::Error::StatusCode(401 | 403) => RegistryError::Unauthorized { name },
        ureq::Error::StatusCode(code) => RegistryError::InvalidResponse {
            name,
            reason: format!("HTTP status {code}"),
        },
        ureq::Error::Timeout(_) => RegistryError::Timeout { name },
        other => RegistryError::Unreachable {
            name,
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};

    use super::*;

    const SCOPED: &str = include_str!("../../tests/fixtures/packuments/@acme/next-plugin.json");

    struct Reply {
        status: u16,
        body: String,
        delay: Duration,
    }

    fn reply(status: u16, body: &str) -> Reply {
        Reply {
            status,
            body: body.to_string(),
            delay: Duration::ZERO,
        }
    }

    // 決まった応答を順に1回ずつ返し、受け取ったリクエストの先頭行と Accept ヘッダーを記録する
    fn serve(replies: Vec<Reply>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("ポートを確保できる");
        let base = format!("http://{}", listener.local_addr().expect("アドレスがある"));
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().expect("接続を受けられる");
                let mut reader = BufReader::new(stream.try_clone().expect("複製できる"));
                let mut head = Vec::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    head.push(line.trim_end().to_string());
                }
                let accept = head
                    .iter()
                    .find(|line| line.to_ascii_lowercase().starts_with("accept:"))
                    .cloned()
                    .unwrap_or_default();
                requests.push(format!(
                    "{} | {accept}",
                    head.first().cloned().unwrap_or_default()
                ));
                thread::sleep(reply.delay);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    reply.body.len(),
                    reply.body
                );
            }
            requests
        });
        (base, handle)
    }

    #[test]
    fn fetches_the_abbreviated_packument_with_the_scoped_name_encoded() {
        let (base, server) = serve(vec![reply(200, SCOPED)]);
        let packument = HttpRegistry::new(&base)
            .packument("@acme/next-plugin")
            .expect("取得できる");

        assert_eq!(packument.versions().len(), 3);
        assert_eq!(
            server.join().expect("サーバーが終わる"),
            [
                "GET /@acme%2fnext-plugin HTTP/1.1 | accept: application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*"
            ]
        );
    }

    #[test]
    fn fetches_each_package_only_once() {
        let (base, server) = serve(vec![reply(200, SCOPED)]);
        let registry = HttpRegistry::new(&base);

        registry.packument("@acme/next-plugin").expect("取得できる");
        registry
            .packument("@acme/next-plugin")
            .expect("2回目は覚えた結果を返す");

        assert_eq!(server.join().expect("サーバーが終わる").len(), 1);
    }

    #[test]
    fn tells_failures_apart() {
        let (base, server) = serve(vec![
            reply(404, "{}"),
            reply(401, "{}"),
            reply(500, "{}"),
            reply(200, "not json"),
        ]);
        let registry = HttpRegistry::new(&base);

        assert!(matches!(
            registry.packument("a"),
            Err(RegistryError::NotFound { .. })
        ));
        assert!(matches!(
            registry.packument("b"),
            Err(RegistryError::Unauthorized { .. })
        ));
        assert!(matches!(
            registry.packument("c"),
            Err(RegistryError::InvalidResponse { reason, .. }) if reason == "HTTP status 500"
        ));
        assert!(matches!(
            registry.packument("d"),
            Err(RegistryError::InvalidResponse { .. })
        ));
        server.join().expect("サーバーが終わる");
    }

    #[test]
    fn times_out_on_a_slow_registry() {
        let (base, server) = serve(vec![Reply {
            status: 200,
            body: SCOPED.to_string(),
            delay: Duration::from_millis(500),
        }]);
        let result =
            HttpRegistry::with_timeout(&base, Duration::from_millis(100)).packument("slow");

        assert!(matches!(result, Err(RegistryError::Timeout { .. })));
        server.join().expect("サーバーが終わる");
    }

    #[test]
    fn reports_an_unreachable_registry() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("ポートを確保できる");
        let base = format!("http://{}", listener.local_addr().expect("アドレスがある"));
        drop(listener);

        assert!(matches!(
            HttpRegistry::new(&base).packument("anything"),
            Err(RegistryError::Unreachable { .. })
        ));
    }

    #[test]
    #[ignore = "本物の npm レジストリに通信する"]
    fn fetches_from_the_public_registry() {
        let packument = HttpRegistry::new("https://registry.npmjs.org")
            .packument("react")
            .expect("取得できる");
        assert!(packument.latest().is_some());
        assert!(!packument.versions().is_empty());
    }
}
