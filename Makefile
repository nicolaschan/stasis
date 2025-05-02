.PHONY: build build-docker run pause stop clean update

all: build

# outputs
BUILD_DIR := build
QEMU_IMAGE := $(BUILD_DIR)/qemu-image
VM_QCOW2 := $(BUILD_DIR)/app.qcow2

# sources
FLAKE_FILE := flake.nix
FLAKE_LOCK := flake.lock
NIX_FILES := $(shell find . -name "*.nix" -type f)
SH_FILES := $(shell find . -name "*.sh" -type f)
RUST_FILES := $(shell find . -name "*.rs" -type f)

# docker config
IMAGE_NAME := qemu-image:latest
CONTAINER_NAME := qemu-container

$(QEMU_IMAGE): $(FLAKE_FILE) $(FLAKE_LOCK) $(NIX_FILES) $(SH_FILES) $(RUST_FILES)
	nix build .#image
	mkdir -p $(BUILD_DIR)
	cp -f -L result $(QEMU_IMAGE)

$(VM_QCOW2): $(FLAKE_FILE) $(FLAKE_LOCK) $(NIX_FILES) $(SH_FILES) $(RUST_FILES)
	nix build .#vm
	mkdir -p $(BUILD_DIR)
	cp -f -L result/nixos.qcow2 $(VM_QCOW2)

build: $(QEMU_IMAGE) $(VM_QCOW2)
	@echo "Build up to date!"

build-docker: clean
	docker run --privileged \
		-v $(PWD):/app \
		-e "NIX_CONFIG=experimental-features = nix-command flakes" \
		nixos/nix \
		bash -c 'cp -r /app /app2 && cd /app2 && nix run nixpkgs#gnumake && cp -r /app2/build /app/build && chown -R 1000 /app/build'

load: build
	docker load < $(QEMU_IMAGE)

run: stop build load
	docker run --privileged --rm -d \
		-p 2222:2222 \
		-p 25560:25560 \
		-v $(PWD)/$(BUILD_DIR):/app/build \
		--name $(CONTAINER_NAME) \
		$(IMAGE_NAME)

run-build: load
	docker run --privileged --rm -d \
		-p 2222:2222 \
		-p 25560:25560 \
		-e "STASIS_AUTO_BUILD_IMAGE=true" \
		--name $(CONTAINER_NAME) \
		$(IMAGE_NAME)

pause:
	-docker exec -it $(CONTAINER_NAME) /bin/stasis-tools snapshot

stop: pause
	@docker stop $(CONTAINER_NAME) 2>/dev/null || true
	@docker rm $(CONTAINER_NAME) 2>/dev/null || true

logs:
	docker logs -f $(CONTAINER_NAME) 2>/dev/null

update:
	nix flake update

clean: stop
	rm -rf result
	rm -rf $(BUILD_DIR)
	-docker rmi $(IMAGE_NAME)
