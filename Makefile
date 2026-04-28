# Nama Image Docker
IMAGE_NAME=crypto-bot
CONTAINER_NAME=trading_instance

.PHONY: build up down restart log clean ps

# Membangun image Docker bot (Rust)
build:
	docker build -t $(IMAGE_NAME) .

# Menjalankan SEMUA service (Bot + PostgreSQL) via Docker Compose
up:
	docker compose up -d

# Menghentikan semua service
down:
	docker compose down

# Restart semua service (Paksa Down & Up)
restart:
	docker compose down && docker compose up -d

# Melihat log bot secara real-time
log:
	docker compose logs -f trading_bot

# Melihat log PostgreSQL
log-db:
	docker compose logs -f postgres

# Melihat status semua container
ps:
	docker compose ps

# Membersihkan image yang tidak terpakai (hati-hati: tidak hapus volume/data DB)
clean:
	docker system prune -f

# Update kode dari Git dan langsung deploy perubahan
update:
	git pull origin $(shell git rev-parse --abbrev-ref HEAD)
	docker compose up -d --build

# Rebuild image dan update service yang berubah saja
deploy:
	docker compose up -d --build
