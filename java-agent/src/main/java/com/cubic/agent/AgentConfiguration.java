package com.cubic.agent;

import java.nio.file.Path;

final class AgentConfiguration {
    static final String OUTPUT_PATH_PROPERTY = "cubic.agent.output.path";
    static final String MODS_CACHE_DIR_PROPERTY = "cubic.agent.mods.cache.dir";

    private final Path outputPath;
    private final Path modsCacheDir;

    private AgentConfiguration(Path outputPath, Path modsCacheDir) {
        this.outputPath = outputPath;
        this.modsCacheDir = modsCacheDir;
    }

    static AgentConfiguration fromSystemProperties() {
        String outputPathValue = System.getProperty(OUTPUT_PATH_PROPERTY);
        String modsCacheDirValue = System.getProperty(MODS_CACHE_DIR_PROPERTY);

        if (outputPathValue == null || outputPathValue.isBlank()) {
            throw new IllegalStateException("Missing system property: " + OUTPUT_PATH_PROPERTY);
        }

        if (modsCacheDirValue == null || modsCacheDirValue.isBlank()) {
            throw new IllegalStateException("Missing system property: " + MODS_CACHE_DIR_PROPERTY);
        }

        return new AgentConfiguration(Path.of(outputPathValue), Path.of(modsCacheDirValue));
    }

    Path outputPath() {
        return outputPath;
    }

    Path modsCacheDir() {
        return modsCacheDir;
    }
}
