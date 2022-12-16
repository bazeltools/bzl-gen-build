package io.bazeltools.buildgen.javadefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: Symbols) =
    assert(JavaSourceEntityExtractor.extract(str).unsafeRunSync() === expected)

  test("can extract a simple file") {
    val simpleContent = """
    package com.foo.bar;

    public class Cat {

    }
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(Entity.dotted("com.foo.bar.Cat")),
      refs = SortedSet(),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols)
  }

  test("can extract a was failing file") {
    val simpleContent = """
   package com.foo.bar;

import com.foo.zeb.Animal;
import com.foo.zeb.Cat;
import com.foo.zeb.Dog;
import com.foo.zeb.Dinosaur;
import com.foo.zeb.Funky;
import com.foo.zeb.FeatureEncoderDef;
import com.foo.zeb.Validate;

import javax.annotation.Nullable;

@Animal
@FeatureEncoderDef("test.ExampleIntegerEncoder")
public interface ExampleIntegerEncoder {

    @Funky(name="result")
    default Integer apply(@Dog("data") @Nullable final Integer data,
                         @Cat("a") @Dinosaur("0") @Validate("$ > 0") final Integer a,
                         @Cat("b") @Dinosaur("1") final Integer b) {
        return (data != null ? data : 0) + a + b;
    }
}
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(Entity.dotted("com.foo.bar.ExampleIntegerEncoder")),
      refs = SortedSet(
        Entity.dotted("Animal"),
        Entity.dotted("Cat"),
        Entity.dotted("Dinosaur"),
        Entity.dotted("Dog"),
        Entity.dotted("FeatureEncoderDef"),
        Entity.dotted("Funky"),
        Entity.dotted("Integer"),
        Entity.dotted("Nullable"),
        Entity.dotted("Validate"),
        Entity.dotted("com.foo.bar.Animal"),
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Dinosaur"),
        Entity.dotted("com.foo.bar.Dog"),
        Entity.dotted("com.foo.bar.FeatureEncoderDef"),
        Entity.dotted("com.foo.bar.Funky"),
        Entity.dotted("com.foo.bar.Integer"),
        Entity.dotted("com.foo.bar.Nullable"),
        Entity.dotted("com.foo.bar.Validate"),
        Entity.dotted("com.foo.zeb.Animal"),
        Entity.dotted("com.foo.zeb.Cat"),
        Entity.dotted("com.foo.zeb.Dinosaur"),
        Entity.dotted("com.foo.zeb.Dog"),
        Entity.dotted("com.foo.zeb.FeatureEncoderDef"),
        Entity.dotted("com.foo.zeb.Funky"),
        Entity.dotted("com.foo.zeb.Validate"),
        Entity.dotted("javax.annotation.Nullable")
      ),
      bzl_gen_build_commands = SortedSet()
    )
    assertParse(simpleContent, expectedSymbols)
  }

}
