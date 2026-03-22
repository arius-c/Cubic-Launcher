package com.cubic.agent;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

final class ConfigAttributionWriter {
    private static final OpenOption[] APPEND_OPTIONS = new OpenOption[] {
        StandardOpenOption.CREATE,
        StandardOpenOption.WRITE,
        StandardOpenOption.APPEND
    };

    private final Path outputPath;

    ConfigAttributionWriter(Path outputPath) {
        this.outputPath = outputPath;
    }

    synchronized void write(AttributionEvent event) {
        try {
            Files.writeString(outputPath, event.toNdjson() + System.lineSeparator(), StandardCharsets.UTF_8, APPEND_OPTIONS);
        } catch (IOException exception) {
            throw new UncheckedIOException("Failed to write attribution event", exception);
        }
    }

    void close() {
    }
}
