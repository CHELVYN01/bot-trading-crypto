# Nama Image Docker
IMAGE_NAME=crypto-bot
CONTAINER_NAME=trading_instance

.PHONY: build run stop log restart clean

# Membangun image Docker (Menggunakan multi-stage build & cargo-chef)
build:
	docker build -t $(IMAGE_NAME) .

# Menjalankan kontainer dengan menyertakan file .env
run:
	docker run -d \
		--name $(CONTAINER_NAME) \
		--env-file .env \
		--restart unless-stopped \
		$(IMAGE_NAME)

# Menghentikan dan menghapus kontainer
stop:
	docker stop $(CONTAINER_NAME) || true
	docker rm $(CONTAINER_NAME) || true

# Melihat log bot secara real-time
log:
	docker logs -f $(CONTAINER_NAME)

# Restart bot (berguna jika ada perubahan konfigurasi di .env)
restart: stop run

# Membersihkan image yang tidak terpakai
clean:
	docker system prune -f
