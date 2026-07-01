.PHONY: docker-image

docker-image:
	export VERSION="$$(date +%Y-%m-%d.%H%M%S)"; \
	docker build \
	-t magonx/konakona:latest \
	-t magonx/konakona:$$VERSION \
	--platform linux/amd64 . && \
	echo "builded magonx/konakona:latest, magonx/konakona:$$VERSION"