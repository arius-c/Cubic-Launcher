package com.cubic.agent;

final class JsonEscaper {
    private JsonEscaper() {
    }

    static String escape(String value) {
        StringBuilder builder = new StringBuilder(value.length() + 8);

        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '\\' -> builder.append("\\\\");
                case '"' -> builder.append("\\\"");
                case '\n' -> builder.append("\\n");
                case '\r' -> builder.append("\\r");
                case '\t' -> builder.append("\\t");
                default -> builder.append(character);
            }
        }

        return builder.toString();
    }
}
