run:
    docker compose up

# update the recipe.json file - this should be done whenever Cargo.lock is changed
chef: 
    cargo chef prepare --recipe-path recipe.json

fmt:
    cargo +nightly fmt
