package org.x.core;

/**
 * A utility class defined in the relocated `core` project (physically under libs/core).
 */
public class Util {

    /** A nested class — imported as `org.x.core.Util.Inner` (suffix-shortens to Util.java). */
    public static class Inner {
        public int value() {
            return 1;
        }
    }

    public String describe() {
        return "util";
    }
}
