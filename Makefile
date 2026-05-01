# Nama Image Docker
IMAGE_NAME=crypto-bot
CONTAINER_NAME=trading_instance

.PHONY: build up down restart log clean ps

# Membangun image Docker bot (Rust)
build:
	docker build -t $(IMAGE_NAME) .

# Menjalankan & build ulang bot saja, lalu langsung lihat log (SOP Update)
up:
	docker compose up -d --build trading_bot
	docker compose logs -f trading_bot

# Menjalankan SEMUA service pertama kali
start:
	docker compose up -d

# Menghentikan semua service
down:
	docker compose down

# Melihat log bot secara real-time
log:
	docker compose logs -f trading_bot

# Update kode dari Git dan langsung deploy bot
update:
	git pull origin develop
	docker compose up -d --build trading_bot
	docker compose logs -f trading_bot

# Melihat status semua container
ps:
	docker compose ps
