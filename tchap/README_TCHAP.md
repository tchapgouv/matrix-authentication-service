## Build 

https://element-hq.github.io/matrix-authentication-service/setup/installation.html#building-from-the-source

frontend 

```
cd frontend
npm ci
npm run build
cd ..
```


policies
```
cd policies
make DOCKER=1
cd ..
```


## To run

```
./start-local-stack.sh
./start-local-mas.sh
```


## Command helper 
```
#createdb -U postgres postgres
# dropdb postgres
#createdb -U postgres -O postgres keycloak
#docker run -d -p 5432:5432 -e 'POSTGRES_USER=keycloak' -e 'POSTGRES_PASSWORD=keycloak' -e 'POSTGRES_DATABASE=keycloak' postgres-keycloak
# cargo test --package mas-handlers upstream_oauth2::link::tests
```