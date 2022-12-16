package io.bazeltools.buildgen.javadefref

import io.bazeltools.buildgen.shared.{DataBlock, Entity}
import scala.collection.immutable.SortedSet

import cats.effect.IO
import io.circe.parser.decode

class JavaSourceEntityExtractorTest extends munit.CatsEffectSuite {

  case class DefsRefs(defs: SortedSet[Entity], refs: SortedSet[Entity])

  def extractString(in: String): IO[DataBlock] =
    JavaSourceEntityExtractor.extract(in)

  def ents(s: String): SortedSet[Entity] =
    decode[List[Entity]](s).map(_.to(SortedSet)) match {
      case Right(ss) => ss
      case Left(err) => sys.error(err.toString)
    }

  def struct(code: String): IO[DefsRefs] =
    extractString(code).map { e => DefsRefs(e.defs, e.refs) }

  test("test basic Java Example") {
    val got = struct("""
    package foo.bar;

    import some.Pack;

    class Bar {
      static class Quux extends Pop {}
      void run(Baz b) {
        call(Bippy.foo);
      }
    }
    """)

    val expected = DefsRefs(
      defs = ents("""["foo.bar.Bar"]"""),
      // TODO: if we call b.call inside a method we shouldn't see b as a dep
      // refs = ents("""["b", "foo.bar.b", "foo.bar.Baz", "Baz", "foo.bar.Pop", "Pop", "Bippy", "foo.bar.Bippy", "some.Pack"]"""))
      refs = ents(
        """["foo.bar.Baz", "Baz", "foo.bar.Pop", "Pop", "Bippy", "foo.bar.Bippy", "some.Pack"]"""
      )
    )

    got.assertEquals(expected)
  }

  /*
  test("test an example adding an edge with a comment") {
    val got = struct("""
    package foo.bar;

    import some.Pack;
    // depgraph:ref:fizzy
    // depgraph:unref:some.Pack

    class Bar { }
    """)

    val expected = DefsRefs(
        defs = ents("""["foo.bar.Bar"]"""),
        refs = ents("""["fizzy"]"""))

    got.assertEquals(expected)
  }
   */

  test("static import") {
    val got = struct("""
    package foo.bar;

    import static foo.bar.Baz.FOO;

    class Bar {
      void myFn() {
        bar();
      }
    }
    """)

    got.map(_.refs(Entity.dotted("foo.bar.Baz"))).assert
  }

  test("static asterisk import") {
    val got = struct("""
    package foo.bar;

    import static foo.bar.Baz.*;

    class Bar {
      void myFn() {
        bar();
      }
    }
    """)

    got.map(_.refs(Entity.dotted("foo.bar.Baz"))).assert
  }

  test("test an annotation") {
    val got = struct("""
    package foo.bar;

    @FeatureEncoderDef("unrelated")
    class BarDef {
      void myFn() {
        bar();
      }
    }
    """)

    got.map(_.defs(Entity.dotted("foo.bar.Bar"))).assert
  }
}
