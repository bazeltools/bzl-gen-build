package io.bazeltools.buildgen.javadefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: Symbols) = {
    val got = JavaSourceEntityExtractor(Set.empty).extract(str).unsafeRunSync()
    assert(got === expected)
  }

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
import com.foo.zeb.Dino.Dog;
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

public class HolderClass {
    @DefaultDataKey(name="pushConsent")
    public static final DataKey<TT> PPPQE = "ABCD";
}
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.ExampleIntegerEncoder"),
        Entity.dotted("com.foo.bar.HolderClass"),
        Entity.dotted("com.foo.bar.MixinDefaultPushConsentKey")
      ),
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
        Entity.dotted("com.foo.zeb.Dino.Dog"),
        Entity.dotted("com.foo.zeb.Dino"),
        Entity.dotted("com.foo.zeb.Dog"),
        Entity.dotted("com.foo.zeb.FeatureEncoderDef"),
        Entity.dotted("com.foo.zeb.Funky"),
        Entity.dotted("com.foo.zeb.Validate"),
        Entity.dotted("javax.annotation.Nullable"),
        Entity.dotted("DataKey"),
        Entity.dotted("DefaultDataKey"),
        Entity.dotted("TT"),
        Entity.dotted("com.foo.bar.DataKey"),
        Entity.dotted("com.foo.bar.DefaultDataKey"),
        Entity.dotted("com.foo.bar.TT")
      ),
      bzl_gen_build_commands = SortedSet()
    )
    assertParse(simpleContent, expectedSymbols)
  }

  test("can extract yield") {
    val simpleContent = """
package com.ducks;

public class Duck {
  public static String quack(int num) {
    String msg = switch(num) {
      case 1 -> { yield "one"; }
      case 2 -> { yield "two"; }
      default -> { yield "lots"; }
    };
    return msg;
  }
}
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(Entity.dotted("com.ducks.Duck")),
      refs =  SortedSet(Entity.dotted("String"), Entity.dotted("com.ducks.String")),
      bzl_gen_build_commands = SortedSet()
    )
    assertParse(simpleContent, expectedSymbols)
  }

}
