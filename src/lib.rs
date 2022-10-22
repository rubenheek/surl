use worker::*;

mod utils;

fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or_else(|| "unknown region".into())
    );
}

#[durable_object]
pub struct ShortUrl {
    state: State,
    env: Env,
}

#[durable_object]
impl DurableObject for ShortUrl {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        let mut router = matchit::Router::new();
        router.insert("/create", ()).unwrap();
        router.insert("/", ()).unwrap();
        let path = req.path();
        let matches = router.at(&path).unwrap();
        if let Some(id) = matches.params.get("id") {
            let url = Url::parse("http://qrl.to/").unwrap();
            self.state.storage().put("to", url).await?;
            Response::ok("created")
        } else {
            let url = self.state.storage().get::<Url>("to").await?;
            let visits = self.state.storage().get::<usize>("visits").await?;
            self.state.storage().put("visits", visits + 1).await?;
            Response::redirect(url)
        }
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);

    utils::set_panic_hook();

    Router::new()
        .get("/", |_, _| Response::ok("Hello from Workers!"))
        .post_async("/create/:id", |req, ctx| async move {
            if let Some(id) = ctx.param("id") {
                let namespace = ctx.durable_object("SHORT_URL")?;
                let stub = namespace.id_from_name(id)?.get_stub()?;
                let mut new_url = req.url()?;
                new_url.set_path("/create");
                return stub.fetch_with_str(new_url.as_str()).await;
            } else {
                Response::error("Bad request", 400)
            }
        })
        .get_async("/q/:id", |req, ctx| async move {
            if let Some(id) = ctx.param("id") {
                let namespace = ctx.durable_object("SHORT_URL")?;
                let stub = namespace.id_from_name(id)?.get_stub()?;
                let mut new_url = req.url()?;
                new_url.set_path("/redirect");
                stub.fetch_with_str(new_url.as_str()).await
            } else {
                Response::error("Bad request", 400)
            }
        })
        .run(req, env)
        .await
}
