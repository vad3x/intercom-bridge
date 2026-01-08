# Use Espressif's pre-built ESP-Matter image (includes Matter SDK)
FROM espressif/esp-matter:latest

ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8
ENV ESP_MATTER_PATH=/opt/espressif/esp-matter
ENV IDF_TARGET=esp32c6

RUN apt-get update -y && apt-get install -y udev

# Source both IDF and Matter exports in bashrc
RUN echo "source /opt/espressif/esp-idf/export.sh > /dev/null 2>&1" >> ~/.bashrc && \
    echo "source /opt/espressif/esp-matter/export.sh > /dev/null 2>&1" >> ~/.bashrc
