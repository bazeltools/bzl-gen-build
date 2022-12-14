package io.bazeltools.buildgen.javadefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: DataBlock) =
    assert(JavaSourceEntityExtractor.extract(str).unsafeRunSync() === expected)

  test("can extract a simple file") {
    val simpleContent = """
    package com.foo.bar;

    public class Cat {

    }
    """
    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(Entity.dotted("com.foo.bar.Cat")),
      refs = SortedSet(),
      bzl_gen_build_commands = SortedSet()
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.ExampleIntegerEncoder -> Animal",
        "link: com.foo.bar.ExampleIntegerEncoder -> Cat",
        "link: com.foo.bar.ExampleIntegerEncoder -> Dinosaur",
        "link: com.foo.bar.ExampleIntegerEncoder -> Dog",
        "link: com.foo.bar.ExampleIntegerEncoder -> FeatureEncoderDef",
        "link: com.foo.bar.ExampleIntegerEncoder -> Funky",
        "link: com.foo.bar.ExampleIntegerEncoder -> Integer",
        "link: com.foo.bar.ExampleIntegerEncoder -> Nullable",
        "link: com.foo.bar.ExampleIntegerEncoder -> Validate",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Animal",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Cat",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Dinosaur",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Dog",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.FeatureEncoderDef",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Funky",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Integer",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Nullable",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.bar.Validate",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Animal",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Cat",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Dinosaur",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Dog",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.FeatureEncoderDef",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Funky",
        "link: com.foo.bar.ExampleIntegerEncoder -> com.foo.zeb.Validate",
        "link: com.foo.bar.ExampleIntegerEncoder -> javax.annotation.Nullable"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("Other usage forms") {
    val simpleContent = """
package com.foo.bar;
import a.b.c.DataKey;
import a.b.e.Kve;

public final class Cat implements Serializable {
    public static final TypeA V0 = null;

    public Cat(final DataKey<Kve> v) {
    }
}
"""

    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat")
      ),
      refs = SortedSet(
        Entity.dotted("DataKey"),
        Entity.dotted("Kve"),
        Entity.dotted("Serializable"),
        Entity.dotted("TypeA"),
        Entity.dotted("a.b.c.DataKey"),
        Entity.dotted("a.b.e.Kve"),
        Entity.dotted("com.foo.bar.DataKey"),
        Entity.dotted("com.foo.bar.Kve"),
        Entity.dotted("com.foo.bar.Serializable"),
        Entity.dotted("com.foo.bar.TypeA")
      ),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.Cat -> com.foo.bar.TypeA",
        "link: com.foo.bar.Cat -> a.b.c.DataKey",
        "link: com.foo.bar.Cat -> DataKey",
        "link: com.foo.bar.Cat -> Kve",
        "link: com.foo.bar.Cat -> Serializable",
        "link: com.foo.bar.Cat -> TypeA",
        "link: com.foo.bar.Cat -> a.b.e.Kve",
        "link: com.foo.bar.Cat -> com.foo.bar.DataKey",
        "link: com.foo.bar.Cat -> com.foo.bar.Kve",
        "link: com.foo.bar.Cat -> com.foo.bar.Serializable"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

}
