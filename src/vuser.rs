use log::{debug, error};
use tokio::sync::mpsc;

use crate::config::{AppConfigHttp, AppConfigHttpReq, AppConfigHttpRes};
use crate::param::ParamResolver;
use crate::transport::{Endpoint, TcpSequence, Transport};

#[derive(Clone)]
pub struct VirtualUser {
    pub id: usize,
    pub tx: mpsc::Sender<VirtualUserCommand>,
    pub client: Endpoint,
}

pub enum VirtualUserCommand {
    HttpRequest(Session),
}

#[derive(Debug)]
pub struct Session {
    req: SampledHttpReq,
    res: SampledHttpRes,
}

#[derive(Debug)]
pub struct SampledHttpReq {
    pub method: String,
    pub path: String,
    pub host: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug)]
pub struct SampledHttpRes {
    pub status: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl VirtualUser {
    pub fn spawn(id: usize, transport: Transport) -> Self {
        let (tx, mut rx) = mpsc::channel(1024);
        let client = transport.allocate_client();

        tokio::spawn(async move {
            let mut sequence = SequenceState::new(id);

            while let Some(command) = rx.recv().await {
                match command {
                    VirtualUserCommand::HttpRequest(session) => {
                        let seq = sequence.next();
                        if let Err(err) = session.request(id, client, seq, transport.clone()).await
                        {
                            error!("vuser={} session failed: {err:#}", id);
                        }
                    }
                }
            }
        });

        Self { id, tx, client }
    }
}

struct SequenceState {
    next_client_seq: u32,
    next_server_seq: u32,
}

impl SequenceState {
    fn new(vuser_id: usize) -> Self {
        let base = (vuser_id as u32).wrapping_mul(1_000_000);
        Self {
            next_client_seq: 1000u32.wrapping_add(base),
            next_server_seq: 5000u32.wrapping_add(base),
        }
    }

    fn next(&mut self) -> TcpSequence {
        let seq = TcpSequence {
            client: self.next_client_seq,
            server: self.next_server_seq,
        };

        self.next_client_seq = self.next_client_seq.wrapping_add(100_000);
        self.next_server_seq = self.next_server_seq.wrapping_add(100_000);

        seq
    }
}

impl Session {
    pub fn new(http: &AppConfigHttp, params: &ParamResolver) -> Self {
        Self {
            req: SampledHttpReq::sample(&http.req, params),
            res: SampledHttpRes::sample(&http.res, params),
        }
    }

    async fn request(
        self,
        vuser_id: usize,
        client: Endpoint,
        seq: TcpSequence,
        transport: Transport,
    ) -> Result<(), anyhow::Error> {
        debug!(
            "vuser={} client={}:{} client_mac={} cseq={} sseq={} {} {} host={} req_headers={} res_status={} res_headers={} req_body_len={} res_body_len={}",
            vuser_id,
            client.ip,
            client.port,
            client.mac,
            seq.client,
            seq.server,
            self.req.method,
            self.req.path,
            self.req.host,
            self.req.headers.len(),
            self.res.status,
            self.res.headers.len(),
            self.req.body.len(),
            self.res.body.len(),
        );

        let req = self.req;
        let res = self.res;

        tokio::task::spawn_blocking(move || {
            transport.send_session_with_client(client, seq, &req, &res)
        })
        .await?
    }
}

impl SampledHttpReq {
    fn sample(req: &AppConfigHttpReq, params: &ParamResolver) -> Self {
        let mut headers = Vec::with_capacity(req.headers.headers.len());

        for (key, value) in &req.headers.headers {
            headers.push((key.clone(), params.render_sample(value)));
        }

        Self {
            method: params.render_sample(&req.headers.method),
            path: params.render_sample(&req.headers.path),
            host: params.render_sample(&req.headers.host),
            headers,
            body: params.render_sample(&req.body),
        }
    }
}

impl SampledHttpRes {
    fn sample(res: &AppConfigHttpRes, params: &ParamResolver) -> Self {
        let mut headers = Vec::with_capacity(res.headers.headers.len());

        for (key, value) in &res.headers.headers {
            headers.push((key.clone(), params.render_sample(value)));
        }

        Self {
            status: params.render_sample(&res.headers.status),
            headers,
            body: params.render_sample(&res.body),
        }
    }
}
