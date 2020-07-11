docker-compose -f rebuild-schema.yml run --rm diesel_cli diesel database reset &&
docker-compose -f rebuild-schema.yml run --rm diesel_cli diesel migration run
