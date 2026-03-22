package com.cubic.agent;

public record AttributionEvent(String configPath, String jarFilename, String sourceClass) {
    public String toNdjson() {
        return "{"
            + "\"config_path\":\"" + JsonEscaper.escape(configPath) + "\"," 
            + "\"jar_filename\":\"" + JsonEscaper.escape(jarFilename) + "\"," 
            + "\"source_class\":"
            + (sourceClass == null ? "null" : "\"" + JsonEscaper.escape(sourceClass) + "\"")
            + "}";
    }
}
