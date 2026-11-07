use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DnsServer {
    pub address: String,
    pub doh: String,
    pub template: String,
}
impl Default for DnsServer {
    fn default() -> Self {
        Self {
            address: String::new(),
            doh: "automatic".into(),
            template: String::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DnsFamily {
    pub manual: bool,
    pub preferred: DnsServer,
    pub alternative: DnsServer,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "PascalCase")]
pub struct DnsSettings {
    pub ipv4: DnsFamily,
    pub ipv6: DnsFamily,
}

impl DnsSettings {
    pub fn validate(&self) -> Result<()> {
        for (v6, family) in [(false, &self.ipv4), (true, &self.ipv6)] {
            if !family.manual {
                continue;
            }
            ensure!(
                !family.preferred.address.trim().is_empty(),
                "Введите предпочитаемый DNS IPv{}",
                if v6 { 6 } else { 4 }
            );
            for server in [&family.preferred, &family.alternative] {
                if server.address.trim().is_empty() {
                    continue;
                }
                let ip: IpAddr = server
                    .address
                    .trim()
                    .parse()
                    .context("Некорректный IP-адрес DNS")?;
                ensure!(
                    ip.is_ipv6() == v6 && !ip.is_unspecified() && !ip.is_multicast(),
                    "Проверьте семейство и адрес DNS"
                );
                ensure!(
                    ["automatic", "manual", "off"].contains(&server.doh.as_str()),
                    "Выберите режим DNS через HTTPS"
                );
                if server.doh == "manual" {
                    let url = url::Url::parse(&server.template).context(
                        "Введите HTTPS-адрес DNS, например https://dns.google/dns-query",
                    )?;
                    ensure!(
                        url.scheme() == "https"
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none()
                            && url.fragment().is_none(),
                        "Для DNS через HTTPS нужен HTTPS-адрес без пароля и фрагмента"
                    );
                }
            }
        }
        Ok(())
    }
    fn servers(&self) -> Vec<DnsServer> {
        [&self.ipv4, &self.ipv6]
            .into_iter()
            .filter(|f| f.manual)
            .flat_map(|f| [&f.preferred, &f.alternative])
            .filter(|s| !s.address.trim().is_empty())
            .cloned()
            .collect()
    }
}
fn automatic_template(ip: &str) -> Option<&'static str> {
    match ip {
        "1.1.1.1" | "1.0.0.1" | "2606:4700:4700::1111" | "2606:4700:4700::1001" => {
            Some("https://cloudflare-dns.com/dns-query")
        }
        "8.8.8.8" | "8.8.4.4" | "2001:4860:4860::8888" | "2001:4860:4860::8844" => {
            Some("https://dns.google/dns-query")
        }
        "9.9.9.9" | "149.112.112.112" | "2620:fe::fe" | "2620:fe::9" => {
            Some("https://dns.quad9.net/dns-query")
        }
        _ => None,
    }
}
fn query(host: &str, kind: u16) -> Result<Vec<u8>> {
    use std::hash::BuildHasher;
    let mut out = (std::collections::hash_map::RandomState::new().hash_one(Instant::now()) as u16)
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(&[1, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    ensure!(host.len() <= 253, "DNS-имя слишком длинное");
    for label in host.trim_end_matches('.').split('.') {
        ensure!(
            !label.is_empty() && label.len() <= 63 && label.is_ascii(),
            "Некорректное DNS-имя"
        );
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&[0, 1]);
    Ok(out)
}
fn word(data: &[u8], i: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        data.get(i..i + 2)
            .context("Короткий DNS-ответ")?
            .try_into()?,
    ))
}
fn skip_name(data: &[u8], p: &mut usize) -> Result<()> {
    for _ in 0..128 {
        let n = *data.get(*p).context("Короткое DNS-имя")?;
        *p += 1;
        if n == 0 {
            return Ok(());
        }
        if n & 0xc0 == 0xc0 {
            ensure!(data.get(*p).is_some(), "Короткий DNS-указатель");
            *p += 1;
            return Ok(());
        }
        ensure!(
            n <= 63 && *p + n as usize <= data.len(),
            "Некорректное DNS-имя"
        );
        *p += n as usize;
    }
    bail!("Некорректный DNS-ответ")
}
fn answers(data: &[u8], request: &[u8]) -> Result<Vec<IpAddr>> {
    ensure!(
        data.len() >= request.len()
            && data[..2] == request[..2]
            && data[2] & 0x80 != 0
            && data[2] & 2 == 0,
        "Некорректный или усечённый DNS-ответ"
    );
    ensure!(
        data[3] & 15 == 0 && word(data, 4)? == 1 && data[12..request.len()] == request[12..],
        "DNS-сервер не разрешил имя"
    );
    let mut p = request.len();
    let mut result = vec![];
    for _ in 0..word(data, 6)? {
        skip_name(data, &mut p)?;
        let kind = word(data, p)?;
        let class = word(data, p + 2)?;
        let len = word(data, p + 8)? as usize;
        p += 10;
        let bytes = data.get(p..p + len).context("Короткая DNS-запись")?;
        if class == 1 {
            match (kind, len) {
                (1, 4) => result.push(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(bytes)?))),
                (28, 16) => result.push(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(bytes)?))),
                _ => {}
            }
        }
        p += len;
    }
    Ok(result)
}
fn udp(server: IpAddr, request: &[u8]) -> Result<Vec<u8>> {
    let socket = UdpSocket::bind(if server.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    })?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    socket.connect(SocketAddr::new(server, 53))?;
    socket.send(request)?;
    let mut data = vec![0; 65535];
    let n = socket.recv(&mut data)?;
    data.truncate(n);
    if data.get(2).is_some_and(|b| b & 2 != 0) {
        let mut tcp =
            TcpStream::connect_timeout(&SocketAddr::new(server, 53), Duration::from_secs(2))?;
        tcp.set_read_timeout(Some(Duration::from_secs(2)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(2)))?;
        tcp.write_all(&(request.len() as u16).to_be_bytes())?;
        tcp.write_all(request)?;
        let mut len = [0; 2];
        tcp.read_exact(&mut len)?;
        data.resize(u16::from_be_bytes(len) as usize, 0);
        tcp.read_exact(&mut data)?;
    }
    Ok(data)
}
fn lookup(server: &DnsServer, host: &str, kind: u16) -> Result<Vec<IpAddr>> {
    let ip: IpAddr = server.address.trim().parse()?;
    let request = query(host, kind)?;
    let template = if server.doh == "manual" {
        Some(server.template.as_str())
    } else if server.doh == "automatic" {
        automatic_template(&ip.to_string())
    } else {
        None
    };
    if let Some(template) = template {
        let secure = (|| -> Result<Vec<IpAddr>> {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&request);
            let get = template.contains("{?dns}") || template.contains("{dns}");
            let address = template
                .replace("{?dns}", &format!("?dns={encoded}"))
                .replace("{dns}", &encoded);
            let url = url::Url::parse(&address)?;
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .resolve(
                    url.host_str().context("Нет DNS-хоста")?,
                    SocketAddr::new(ip, url.port_or_known_default().unwrap_or(443)),
                )
                .build()?;
            let req = if get {
                client.get(url)
            } else {
                client
                    .post(url)
                    .header("Content-Type", "application/dns-message")
                    .body(request.clone())
            };
            let response = req
                .header("Accept", "application/dns-message")
                .send()?
                .error_for_status()?;
            let mut data = Vec::new();
            response.take(65536).read_to_end(&mut data)?;
            answers(&data, &request)
        })();
        if secure.is_ok() || server.doh == "manual" {
            return secure;
        }
    }
    answers(&udp(ip, &request)?, &request)
}
type Cache = Mutex<HashMap<String, (Instant, Vec<IpAddr>)>>;
struct Resolver {
    servers: Vec<DnsServer>,
    cache: Cache,
}
impl Resolver {
    fn resolve(&self, host: &str) -> Result<Vec<IpAddr>> {
        if let Ok(ip) = host.parse() {
            return Ok(vec![ip]);
        }
        if host.eq_ignore_ascii_case("localhost") {
            return Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
        }
        if let Some((time, ips)) = self.cache.lock().unwrap().get(host) {
            if time.elapsed() < Duration::from_secs(30) {
                return Ok(ips.clone());
            }
        }
        for server in &self.servers {
            // Query both record families through the chosen server transport.
            let ips = std::thread::scope(|scope| {
                let a = scope.spawn(|| lookup(server, host, 1));
                let b = lookup(server, host, 28);
                let mut ips = a.join().unwrap_or_else(|_| Ok(vec![])).unwrap_or_default();
                ips.extend(b.unwrap_or_default());
                ips
            });
            if !ips.is_empty() {
                let mut cache = self.cache.lock().unwrap();
                if cache.len() >= 1024 {
                    cache.clear();
                }
                cache.insert(host.into(), (Instant::now(), ips.clone()));
                return Ok(ips);
            }
        }
        bail!("DNS-серверы не разрешили имя {host}")
    }
}
fn tunnel(mut local: TcpStream, resolver: &Resolver) -> Result<()> {
    local.set_read_timeout(Some(Duration::from_secs(120)))?;
    local.set_write_timeout(Some(Duration::from_secs(15)))?;
    let mut hello = [0; 2];
    local.read_exact(&mut hello)?;
    ensure!(hello[0] == 5 && hello[1] > 0, "Не SOCKS5");
    let mut methods = vec![0; hello[1] as usize];
    local.read_exact(&mut methods)?;
    ensure!(methods.contains(&0), "Неизвестный SOCKS5-метод");
    local.write_all(&[5, 0])?;
    let mut head = [0; 4];
    local.read_exact(&mut head)?;
    ensure!(
        head[..3] == [5, 1, 0],
        "Поддерживается только SOCKS5 CONNECT"
    );
    let host = match head[3] {
        1 => {
            let mut b = [0; 4];
            local.read_exact(&mut b)?;
            Ipv4Addr::from(b).to_string()
        }
        4 => {
            let mut b = [0; 16];
            local.read_exact(&mut b)?;
            Ipv6Addr::from(b).to_string()
        }
        3 => {
            let mut len = [0];
            local.read_exact(&mut len)?;
            let mut b = vec![0; len[0] as usize];
            local.read_exact(&mut b)?;
            String::from_utf8(b)?
        }
        _ => bail!("Неизвестное семейство SOCKS5"),
    };
    let mut port = [0; 2];
    local.read_exact(&mut port)?;
    let port = u16::from_be_bytes(port);
    let result = (|| -> Result<TcpStream> {
        for ip in resolver.resolve(&host)? {
            if let Ok(remote) =
                TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_secs(4))
            {
                return Ok(remote);
            }
        }
        bail!("Не удалось подключиться")
    })();
    let mut remote = match result {
        Ok(r) => r,
        Err(e) => {
            local.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0])?;
            return Err(e);
        }
    };
    remote.set_read_timeout(Some(Duration::from_secs(120)))?;
    remote.set_write_timeout(Some(Duration::from_secs(15)))?;
    local.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
    let mut other_local = local.try_clone()?;
    let mut other_remote = remote.try_clone()?;
    let upstream = std::thread::spawn(move || {
        let _ = std::io::copy(&mut other_local, &mut other_remote);
        let _ = other_remote.shutdown(Shutdown::Write);
    });
    let _ = std::io::copy(&mut remote, &mut local);
    let _ = local.shutdown(Shutdown::Both);
    let _ = remote.shutdown(Shutdown::Both);
    let _ = upstream.join();
    Ok(())
}
/// Only site WebViews use this loopback SOCKS5 endpoint; TLS is never decrypted.
pub fn start(settings: &DnsSettings) -> Result<Option<SocketAddr>> {
    settings.validate()?;
    let servers = settings.servers();
    if servers.is_empty() {
        return Ok(None);
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let resolver = Arc::new(Resolver {
        servers,
        cache: Mutex::new(HashMap::new()),
    });
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if active.load(Ordering::Relaxed) >= 128 {
                continue;
            }
            active.fetch_add(1, Ordering::Relaxed);
            let active = active.clone();
            let resolver = resolver.clone();
            std::thread::spawn(move || {
                let _ = tunnel(stream, &resolver);
                active.fetch_sub(1, Ordering::Relaxed);
            });
        }
    });
    Ok(Some(address))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires Internet access to an actual DoH resolver"]
    fn live_doh_resolves_ipv4_and_ipv6_without_system_dns() {
        let server = DnsServer {
            address: "1.1.1.1".into(),
            doh: "manual".into(),
            template: "https://cloudflare-dns.com/dns-query".into(),
        };
        assert!(
            lookup(&server, "example.com", 1)
                .unwrap()
                .iter()
                .any(IpAddr::is_ipv4)
        );
        assert!(
            lookup(&server, "example.com", 28)
                .unwrap()
                .iter()
                .any(IpAddr::is_ipv6)
        );
        let settings = DnsSettings {
            ipv4: DnsFamily {
                manual: true,
                preferred: server,
                alternative: DnsServer::default(),
            },
            ipv6: DnsFamily::default(),
        };
        let proxy = start(&settings).unwrap().unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .proxy(reqwest::Proxy::all(format!("socks5h://{proxy}")).unwrap())
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        assert!(
            client
                .get("https://example.com/")
                .send()
                .unwrap()
                .status()
                .is_success()
        );
    }
    #[test]
    fn validates_family_and_secure_template() {
        let mut s = DnsSettings::default();
        s.ipv4.manual = true;
        assert!(s.validate().is_err());
        s.ipv4.preferred.address = "1.1.1.1".into();
        assert!(s.validate().is_ok());
        s.ipv4.preferred.address = "::1".into();
        assert!(s.validate().is_err());
        s.ipv4.preferred.address = "1.1.1.1".into();
        s.ipv4.preferred.doh = "manual".into();
        s.ipv4.preferred.template = "http://dns.test/".into();
        assert!(s.validate().is_err());
        s.ipv4.preferred.template = "https://dns.test/dns-query{?dns}".into();
        assert!(s.validate().is_ok());
    }
    #[test]
    fn parses_dns_and_rejects_wrong_transaction_or_truncation() {
        let q = query("example.test", 1).unwrap();
        let mut r = q.clone();
        r[2] = 0x81;
        r[3] = 0x80;
        r[7] = 1;
        r.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 127, 0, 0, 2]);
        assert_eq!(
            answers(&r, &q).unwrap(),
            vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2))]
        );
        r[0] ^= 1;
        assert!(answers(&r, &q).is_err());
        r[0] ^= 1;
        r.truncate(r.len() - 1);
        assert!(answers(&r, &q).is_err());
    }
    #[test]
    fn socks5_relays_a_real_local_connection() {
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = origin.local_addr().unwrap().port();
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = proxy.local_addr().unwrap();
        let origin_thread = std::thread::spawn(move || {
            let (mut s, _) = origin.accept().unwrap();
            let mut b = [0; 4];
            s.read_exact(&mut b).unwrap();
            assert_eq!(&b, b"ping");
            s.write_all(b"pong").unwrap();
        });
        let proxy_thread = std::thread::spawn(move || {
            let (s, _) = proxy.accept().unwrap();
            tunnel(
                s,
                &Resolver {
                    servers: vec![],
                    cache: Mutex::new(HashMap::new()),
                },
            )
            .unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(&[5, 1, 0]).unwrap();
        let mut hello = [0; 2];
        client.read_exact(&mut hello).unwrap();
        assert_eq!(hello, [5, 0]);
        let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
        request.extend_from_slice(&port.to_be_bytes());
        client.write_all(&request).unwrap();
        let mut reply = [0; 10];
        client.read_exact(&mut reply).unwrap();
        assert_eq!(reply[1], 0);
        client.write_all(b"ping").unwrap();
        let mut reply = [0; 4];
        client.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"pong");
        drop(client);
        origin_thread.join().unwrap();
        proxy_thread.join().unwrap();
    }
}
