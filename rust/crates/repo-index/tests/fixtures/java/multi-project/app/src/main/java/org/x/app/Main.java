package org.x.app;

import org.x.core.Util;
import org.x.core.Util.Inner;
import org.x.core.*;

public class Main {
    public static void main(String[] args) {
        Util u = new Util();
        Inner i = new Inner();
        System.out.println(u.describe() + i.value());
    }
}
