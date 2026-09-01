FROM registry.gitlab.com/secondstarcavalier/schematic/containers/debian/schematic-debian-1-2:latest
COPY target/release/messaging /usr/local/bin/messaging
ENTRYPOINT ["messaging"]
