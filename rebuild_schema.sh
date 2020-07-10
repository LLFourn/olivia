docker-compose -f rebuild-schema.yml run --rm diesel_cli diesel setup &&
docker-compose -f rebuild-schema.yml run --rm diesel_cli diesel migration run
