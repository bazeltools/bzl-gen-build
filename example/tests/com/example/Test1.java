package com.example;

import static org.junit.Assert.assertEquals;

import org.junit.Test;
import com.example.A;
import com.example.foo.B;

public class Test1 {
  @Test
  public void testA() throws Exception {
    A a = A.newBuilder()
      .addTags("foo")
      .build();
    assertEquals(a.getTags(0), "foo");
  }
}
