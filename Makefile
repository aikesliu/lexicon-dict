.PHONY: build app release dist ubuntu test clean install

build: release

app release:
	cargo build --release -p lexicon-app

test:
	cargo test --workspace

dist:
	./packaging/build.sh

ubuntu:
	./packaging/ubuntu/build.sh

install: release
	./packaging/install.sh

clean:
	cargo clean
	rm -rf dist
