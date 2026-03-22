package com.cubic.agent;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.instrument.Instrumentation;
import java.nio.file.Files;

import net.bytebuddy.agent.builder.AgentBuilder;

public final class ConfigAttributionAgent {
    private ConfigAttributionAgent() {
    }

    public static void premain(String agentArgs, Instrumentation instrumentation) {
        install(instrumentation);
    }

    public static void agentmain(String agentArgs, Instrumentation instrumentation) {
        install(instrumentation);
    }

    private static void install(Instrumentation instrumentation) {
        AgentConfiguration configuration = AgentConfiguration.fromSystemProperties();
        ensureOutputFileExists(configuration);

        ConfigAttributionWriter writer = new ConfigAttributionWriter(configuration.outputPath());
        Runtime.getRuntime().addShutdownHook(new Thread(writer::close));

        new AgentBuilder.Default()
            .ignore((typeDescription, classLoader, module, classBeingRedefined, protectionDomain) -> false)
            .with(AgentBuilder.Listener.NoOp.INSTANCE)
            .installOn(instrumentation);
    }

    private static void ensureOutputFileExists(AgentConfiguration configuration) {
        try {
            if (configuration.outputPath().getParent() != null) {
                Files.createDirectories(configuration.outputPath().getParent());
            }
            if (Files.notExists(configuration.outputPath())) {
                Files.createFile(configuration.outputPath());
            }
        } catch (IOException exception) {
            throw new UncheckedIOException("Failed to prepare attribution output file", exception);
        }
    }
}
