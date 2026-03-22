plugins {
    id("java")
}

group = "com.cubic"
version = "0.1.0"

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(21))
    }
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.bytebuddy:byte-buddy:1.17.8")
    implementation("net.bytebuddy:byte-buddy-agent:1.17.8")
}

tasks.jar {
    manifest {
        attributes(
            "Premain-Class" to "com.cubic.agent.ConfigAttributionAgent",
            "Agent-Class" to "com.cubic.agent.ConfigAttributionAgent",
            "Can-Redefine-Classes" to "true",
            "Can-Retransform-Classes" to "true"
        )
    }

    from(configurations.runtimeClasspath.get().map { if (it.isDirectory) it else zipTree(it) })
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
}
