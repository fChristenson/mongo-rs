use actix_web::middleware::Logger;
use actix_web::{get, web, App, Error, HttpResponse, HttpServer};
use bson::{doc, Bson};
use env_logger::Env;
use listenfd::ListenFd;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use uuid;

#[derive(Serialize, Deserialize, Debug)]
struct Foo {
    id: String,
    test: i32,
}

impl Foo {
    pub fn new(test: i32) -> Foo {
        let uuid = uuid::Uuid::new_v4().to_hyphenated();
        Foo {
            id: uuid.to_string(),
            test,
        }
    }
}

#[derive(Clone)]
struct TestDao {
    pub collection: mongodb::Collection,
}

impl TestDao {
    pub async fn new() -> Result<TestDao, mongodb::error::Error> {
        let client = Client::with_uri_str("mongodb://localhost:27017").await?;
        let db = client.database("test");
        let collection = db.collection("products");

        Ok(TestDao { collection })
    }

    pub async fn save(&self, foo: &Foo) -> Result<(), mongodb::error::Error> {
        let to_save = doc! {
            "id": foo.id.clone(),
            "test": foo.test
        };
        self.collection.insert_one(to_save, None).await?;
        Ok(())
    }

    pub async fn find_by_id(&self, id: &String) -> Result<Option<Foo>, Box<dyn std::error::Error>> {
        let query = doc! {"id": id};
        let found: Option<Bson> = self
            .collection
            .find_one(query, None)
            .await?
            .map(|f| f.into());

        let json: Option<serde_json::Value> = found.map(|f| f.into());
        let foo = match json {
            Some(json) => {
                let foo: Foo = serde_json::from_value(json)?;
                Some(foo)
            }
            _ => None,
        };

        Ok(foo)
    }
}

#[derive(Clone)]
struct FooService {
    test_dao: TestDao,
}

impl FooService {
    pub async fn new() -> Result<FooService, mongodb::error::Error> {
        let test_dao = TestDao::new().await?;
        Ok(FooService { test_dao })
    }

    pub async fn save(&self, foo: &Foo) -> Result<(), mongodb::error::Error> {
        self.test_dao.save(foo).await
    }

    pub async fn find_by_id(&self, id: &String) -> Result<Option<Foo>, Box<dyn std::error::Error>> {
        self.test_dao.find_by_id(&id).await
    }
}

#[get("/number/{number}")]
async fn index(data: web::Data<State>, path: web::Path<(i32,)>) -> Result<HttpResponse, Error> {
    let foo = Foo::new(path.0);
    data.foo_service.save(&foo).await.expect("Failed to save");

    let response_foo = data
        .foo_service
        .find_by_id(&foo.id)
        .await
        .expect("Could not find foo");
    Ok(HttpResponse::Ok().json(response_foo))
}

#[derive(Clone)]
struct State {
    foo_service: FooService,
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    let state = State {
        foo_service: FooService::new().await.expect("Foo failed"),
    };

    let mut listenfd = ListenFd::from_env();
    let mut server = HttpServer::new(move || {
        App::new()
            .data(state.clone())
            .wrap(Logger::default())
            .service(index)
    });

    server = if let Some(l) = listenfd.take_tcp_listener(0).unwrap() {
        server.listen(l)?
    } else {
        server.bind("127.0.0.1:8080")?
    };

    server.run().await
}
