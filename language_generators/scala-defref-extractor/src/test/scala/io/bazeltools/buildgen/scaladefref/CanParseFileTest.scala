package io.bazeltools.buildgen.scaladefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: Symbols, specialTlds: List[String]) = {
    val map = Entity.makeSpecialTldsMap(specialTlds)
    val got = ScalaSourceEntityExtractor(map).extract(str).unsafeRunSync()
    assert(got === expected)
  }

  test("can extract a simple file") {
    val simpleContent = """
    package com.foo.bar

    case class Cat(foo: String)
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.foo")
      ),
      refs =
        SortedSet(Entity.dotted("String"), Entity.dotted("com.foo.bar.String")),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("can extract a strange syntax") {
    val simpleContent = """
    package com.foo.bar
    import com.example.Wolf
    import com.example.Elephant

    case class Cat(foo: String) extends Wolf with Elephant {
        val expressionName = Dog()
        // This format means we will throw a match error if Lizard != Dog with match
        val (`expressionName`, pie) = (Lizard(), 33)
    }
    """

    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.expressionName"),
        Entity.dotted("com.foo.bar.Cat.foo"),
        Entity.dotted("com.foo.bar.Cat.pie")
      ),
      SortedSet(
        Entity.dotted("Dog"),
        Entity.dotted("Lizard"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.Dog"),
        Entity.dotted("com.foo.bar.Lizard"),
        Entity.dotted("com.foo.bar.String"),
        Entity.dotted("com"),
        Entity.dotted("com.example"),
        Entity.dotted("com.example.Elephant"),
        Entity.dotted("com.example.Wolf")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("can extract a stranger syntax") {
    val simpleContent = """
    package com.foo.bar
    import com.example.Wolf
    import com.example.Elephant

    case class Cat(foo: String) extends Wolf with Elephant {
        val expressionName = Dog()
        // This format means we will throw a match error if Lizard != Dog with match
        val (`expressionName`, pie) = (Lizard(), 33)
    }
    """

    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.expressionName"),
        Entity.dotted("com.foo.bar.Cat.foo"),
        Entity.dotted("com.foo.bar.Cat.pie")
      ),
      SortedSet(
        Entity.dotted("Dog"),
        Entity.dotted("Lizard"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.Dog"),
        Entity.dotted("com.foo.bar.Lizard"),
        Entity.dotted("com.foo.bar.String"),
        Entity.dotted("com"),
        Entity.dotted("com.example"),
        Entity.dotted("com.example.Elephant"),
        Entity.dotted("com.example.Wolf"),
        Entity.dotted("com.foo.bar.com"),
        Entity.dotted("com.foo.bar.com.example"),
        Entity.dotted("com.foo.bar.com.example.Elephant"),
        Entity.dotted("com.foo.bar.com.example.Wolf")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, Nil)
  }

  test("can extract a failing file") {
    val simpleContent = """
    package com.foo.bar

    import z.FooBarZ
    object TestObj extends FooBarZ {
        def test(typ: TensorDataType) = forAll(genSparseTensorOf(typ)) {
        tensor =>
          val elementType = CustomObject.ElementType(typ)
          val expectedSparkType = CustomObject.SparseType(elementType)
          val (`expectedSparkType`, metadata, writer) = CustomObject.writer(tensor.getFlavor, tensor.getDataType, tensor.getShape)
          val reader = CustomObject.sparseReader(elementType, tensor.getShape)
          val read = reader(writer(tensor).asInstanceOf[InternalRow])

          if (read != tensor) {
            println("Tensors don't match")
          }

          read shouldEqual tensor
      }
    }
    """
    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.foo.bar.TestObj"),
        Entity.dotted("com.foo.bar.TestObj.test")
      ),
      SortedSet(
        Entity.dotted("CustomObject"),
        Entity.dotted("CustomObject.ElementType"),
        Entity.dotted("CustomObject.SparseType"),
        Entity.dotted("CustomObject.sparseReader"),
        Entity.dotted("CustomObject.writer"),
        Entity.dotted("InternalRow"),
        Entity.dotted("TensorDataType"),
        Entity.dotted("com.foo.bar.CustomObject"),
        Entity.dotted("com.foo.bar.CustomObject.ElementType"),
        Entity.dotted("com.foo.bar.CustomObject.SparseType"),
        Entity.dotted("com.foo.bar.CustomObject.sparseReader"),
        Entity.dotted("com.foo.bar.CustomObject.writer"),
        Entity.dotted("com.foo.bar.InternalRow"),
        Entity.dotted("com.foo.bar.TensorDataType"),
        Entity.dotted("com.foo.bar.forAll"),
        Entity.dotted("com.foo.bar.genSparseTensorOf"),
        Entity.dotted("com.foo.bar.println"),
        Entity.dotted("forAll"),
        Entity.dotted("genSparseTensorOf"),
        Entity.dotted("println"),
        Entity.dotted("com.foo.bar.z"),
        Entity.dotted("com.foo.bar.z.FooBarZ"),
        Entity.dotted("z"),
        Entity.dotted("z.FooBarZ")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("can extract a failing file 2") {
    val simpleContent = """
package com.foo.bar
import z.{GenA, GenB, NP, OT}
import org.apache.spark.sql.functions.udf
trait Zeb {
    def np: NP
}
trait TestTrait extends GenA with GenB with Zeb{
  def foo(): Long = {
      combinedParams.underlying.collect {
        case (param: OtherTrait[param], value: String) if outCols.contains(param) => (param, value)
      }
  }
    val getCountryCodeUdf = udf(getCountryCode _)

  val myLocalV = new OtherClass.OtherSubClass()
  def tstFn = Dependency
      .on(OT)
      .fff('foo -> foo)
}
    """
    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.foo.bar.TestTrait"),
        Entity.dotted("com.foo.bar.TestTrait.foo"),
        Entity.dotted("com.foo.bar.Zeb"),
        Entity.dotted("com.foo.bar.Zeb.np"),
        Entity.dotted("com.foo.bar.TestTrait.getCountryCodeUdf"),
        Entity.dotted("com.foo.bar.TestTrait.tstFn"),
        Entity.dotted("com.foo.bar.TestTrait.myLocalV")
      ),
      SortedSet(
        Entity.dotted("Long"),
        Entity.dotted("OtherTrait"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.Long"),
        Entity.dotted("com.foo.bar.OtherTrait"),
        Entity.dotted("com.foo.bar.String"),
        Entity.dotted("com.foo.bar.combinedParams"),
        Entity.dotted("com.foo.bar.combinedParams.underlying"),
        Entity.dotted("com.foo.bar.combinedParams.underlying.collect"),
        Entity.dotted("com.foo.bar.outCols"),
        Entity.dotted("com.foo.bar.outCols.contains"),
        Entity.dotted("com.foo.bar.param"),
        Entity.dotted("com.foo.bar.value"),
        Entity.dotted("combinedParams"),
        Entity.dotted("combinedParams.underlying"),
        Entity.dotted("combinedParams.underlying.collect"),
        Entity.dotted("outCols"),
        Entity.dotted("outCols.contains"),
        Entity.dotted("param"),
        Entity.dotted("value"),
        Entity.dotted("com.foo.bar.z"),
        Entity.dotted("com.foo.bar.z.GenA"),
        Entity.dotted("com.foo.bar.z.GenB"),
        Entity.dotted("z"),
        Entity.dotted("z.GenA"),
        Entity.dotted("z.GenB"),
        Entity.dotted("com.foo.bar.z.NP"),
        Entity.dotted("z.NP"),
        Entity.dotted("com.foo.bar.getCountryCode"),
        Entity.dotted("com.foo.bar.org"),
        Entity.dotted("com.foo.bar.org.apache"),
        Entity.dotted("com.foo.bar.org.apache.spark"),
        Entity.dotted("com.foo.bar.org.apache.spark.sql"),
        Entity.dotted("com.foo.bar.org.apache.spark.sql.functions"),
        Entity.dotted("com.foo.bar.org.apache.spark.sql.functions.udf"),
        Entity.dotted("getCountryCode"),
        Entity.dotted("org"),
        Entity.dotted("org.apache"),
        Entity.dotted("org.apache.spark"),
        Entity.dotted("org.apache.spark.sql"),
        Entity.dotted("org.apache.spark.sql.functions"),
        Entity.dotted("org.apache.spark.sql.functions.udf"),
        Entity.dotted("Dependency"),
        Entity.dotted("Dependency.on"),
        Entity.dotted("com.foo.bar.Dependency"),
        Entity.dotted("com.foo.bar.Dependency.on"),
        Entity.dotted("z.OT"),
        Entity.dotted("com.foo.bar.z.OT"),
        Entity.dotted("OtherClass"),
        Entity.dotted("OtherClass.OtherSubClass"),
        Entity.dotted("OtherSubClass"),
        Entity.dotted("com.foo.bar.OtherClass"),
        Entity.dotted("com.foo.bar.OtherClass.OtherSubClass"),
        Entity.dotted("com.foo.bar.OtherSubClass")
      ),
      SortedSet.empty
    )

    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Add transitive links") {
    val simpleContent = """
        package com.foo.bar

        case class Cat(foo: String) extends Dog
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.foo")
      ),
      refs = SortedSet(
        Entity.dotted("Dog"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.Dog"),
        Entity.dotted("com.foo.bar.String")
      ),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Class arg") {
    val simpleContent = """
        package com.foo.bar

        class Cat(dog: Dog) {
        }
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.dog")
      ),
      refs = SortedSet(
        Entity.dotted("Dog"),
        Entity.dotted("com.foo.bar.Dog")
      ),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Add more transitive links") {
    val simpleContent = """
        package com.foo.bar

  sealed abstract class AbstractC
    extends BaseType[BaseTpeParamA, BaseTpeParamB, BaseTpeParamC]{}
    """
    val expectedSymbols = Symbols(
      SortedSet(Entity.dotted("com.foo.bar.AbstractC")),
      SortedSet(
        Entity.dotted("BaseTpeParamA"),
        Entity.dotted("BaseTpeParamB"),
        Entity.dotted("BaseTpeParamC"),
        Entity.dotted("BaseType"),
        Entity.dotted("com.foo.bar.BaseTpeParamA"),
        Entity.dotted("com.foo.bar.BaseTpeParamB"),
        Entity.dotted("com.foo.bar.BaseTpeParamC"),
        Entity.dotted("com.foo.bar.BaseType")
      ),
      SortedSet.empty
    )

    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Add trait transitive links") {
    val simpleContent = """
package com.example
import com.animal.dogs.retriever._
import com.animal.dogs.pugs.{ Small, Cute }
import com.animal.cats.tiger.TigerStripes
import com.animal.cats.housecat.Cuddle
import com.animal.cats.big.BaseTrainingNode

trait BaseNode
    extends CaseClassConfig[TigerStripes]
    with BaseTrainingNode
    with JsonEncoder
    with Express {

        }    """
    val expectedSymbols = Symbols(
      SortedSet(Entity.dotted("com.example.BaseNode")),
      SortedSet(
        Entity.dotted("CaseClassConfig"),
        Entity.dotted("Express"),
        Entity.dotted("JsonEncoder"),
        Entity.dotted("com"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.cats"),
        Entity.dotted("com.animal.cats.big"),
        Entity.dotted("com.animal.cats.big.BaseTrainingNode"),
        Entity.dotted("com.animal.cats.housecat"),
        Entity.dotted("com.animal.cats.housecat.Cuddle"),
        Entity.dotted("com.animal.cats.tiger"),
        Entity.dotted("com.animal.cats.tiger.TigerStripes"),
        Entity.dotted("com.animal.dogs"),
        Entity.dotted("com.animal.dogs.pugs"),
        Entity.dotted("com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.CaseClassConfig"),
        Entity.dotted("com.animal.dogs.retriever.Express"),
        Entity.dotted("com.animal.dogs.retriever.JsonEncoder"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.example.CaseClassConfig"),
        Entity.dotted("com.example.Express"),
        Entity.dotted("com.example.JsonEncoder")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Add trait transitive links, no .com TLD") {
    val simpleContent = """
package com.example
import com.animal.dogs.retriever._
import com.animal.dogs.pugs.{ Small, Cute }
import com.animal.cats.tiger.TigerStripes
import com.animal.cats.housecat.Cuddle
import com.animal.cats.big.BaseTrainingNode

trait BaseNode
    extends CaseClassConfig[TigerStripes]
    with BaseTrainingNode
    with JsonEncoder
    with Express {

        }    """
    val expectedSymbols = Symbols(
      SortedSet(Entity.dotted("com.example.BaseNode")),
      SortedSet(
        Entity.dotted("CaseClassConfig"),
        Entity.dotted("Express"),
        Entity.dotted("JsonEncoder"),
        Entity.dotted("com"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.cats"),
        Entity.dotted("com.animal.cats.big"),
        Entity.dotted("com.animal.cats.big.BaseTrainingNode"),
        Entity.dotted("com.animal.cats.housecat"),
        Entity.dotted("com.animal.cats.housecat.Cuddle"),
        Entity.dotted("com.animal.cats.tiger"),
        Entity.dotted("com.animal.cats.tiger.TigerStripes"),
        Entity.dotted("com.animal.dogs"),
        Entity.dotted("com.animal.dogs.pugs"),
        Entity.dotted("com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.CaseClassConfig"),
        Entity.dotted("com.animal.dogs.retriever.Express"),
        Entity.dotted("com.animal.dogs.retriever.JsonEncoder"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.example.CaseClassConfig"),
        Entity.dotted("com.example.Express"),
        Entity.dotted("com.example.JsonEncoder"),
        Entity.dotted("com.animal.dogs.retriever.com"),
        Entity.dotted("com.animal.dogs.retriever.com.animal"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats.big"),
        Entity.dotted(
          "com.animal.dogs.retriever.com.animal.cats.big.BaseTrainingNode"
        ),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats.housecat"),
        Entity.dotted(
          "com.animal.dogs.retriever.com.animal.cats.housecat.Cuddle"
        ),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats.tiger"),
        Entity.dotted(
          "com.animal.dogs.retriever.com.animal.cats.tiger.TigerStripes"
        ),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.retriever"),
        Entity.dotted("com.example.com"),
        Entity.dotted("com.example.com.animal"),
        Entity.dotted("com.example.com.animal.cats"),
        Entity.dotted("com.example.com.animal.cats.big"),
        Entity.dotted("com.example.com.animal.cats.big.BaseTrainingNode"),
        Entity.dotted("com.example.com.animal.cats.housecat"),
        Entity.dotted("com.example.com.animal.cats.housecat.Cuddle"),
        Entity.dotted("com.example.com.animal.cats.tiger"),
        Entity.dotted("com.example.com.animal.cats.tiger.TigerStripes"),
        Entity.dotted("com.example.com.animal.dogs"),
        Entity.dotted("com.example.com.animal.dogs.pugs"),
        Entity.dotted("com.example.com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.example.com.animal.dogs.pugs.Small"),
        Entity.dotted("com.example.com.animal.dogs.retriever")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, Nil)
  }

  test("Add object transitive links") {
    val simpleContent = """
package com.example
import com.animal.dogs.retriever._
import com.animal.dogs.pugs.{ Small, Cute }
import com.animal.cats.tiger.TigerStripes
import com.animal.cats.housecat.Cuddle
import com.animal.cats.big.BaseTrainingNode

object BaseNode
    extends CaseClassConfig[TigerStripes]
    with BaseTrainingNode
    with JsonEncoder
    with Express {

        }    """
    val expectedSymbols = Symbols(
      SortedSet(Entity.dotted("com.example.BaseNode")),
      SortedSet(
        Entity.dotted("CaseClassConfig"),
        Entity.dotted("Express"),
        Entity.dotted("JsonEncoder"),
        Entity.dotted("com"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.cats"),
        Entity.dotted("com.animal.cats.big"),
        Entity.dotted("com.animal.cats.big.BaseTrainingNode"),
        Entity.dotted("com.animal.cats.housecat"),
        Entity.dotted("com.animal.cats.housecat.Cuddle"),
        Entity.dotted("com.animal.cats.tiger"),
        Entity.dotted("com.animal.cats.tiger.TigerStripes"),
        Entity.dotted("com.animal.dogs"),
        Entity.dotted("com.animal.dogs.pugs"),
        Entity.dotted("com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.CaseClassConfig"),
        Entity.dotted("com.animal.dogs.retriever.Express"),
        Entity.dotted("com.animal.dogs.retriever.JsonEncoder"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.example.CaseClassConfig"),
        Entity.dotted("com.example.Express"),
        Entity.dotted("com.example.JsonEncoder")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Add public method link") {
    val simpleContent = """
package com.example
import com.animal.dogs.retriever.Bar
import com.animal.dogs.gamma.Square
import z.TypeA
import z.TypeG
import z.TypeH
import z.TypeJ
import z.TypeK
import z.TypeL

case class BaseNode() {
    def myFunction(square: Square): Bar = {
        ???
    }
    @Option(foo,bar,baz)
    val foo: Data[TypeA, TypeB[TypeC]] = "asdf"
    @Option(foo,bar,baz)
    var bar = Data[TypeE, TypeF[TypeG]]("asdf") & TypeH
    @Encoder
    val e: FeC[_ with TypeJ] = TRRF.config
        .withId("bar")
        .withSomethingElse(Zed.K)

    val localV = durationWindow match {
      case Some(7) =>
        z.TypeK(
          playDurationWindow = TypeL
        )
    }
}
"""
    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.example.BaseNode"),
        Entity.dotted("com.example.BaseNode.myFunction"),
        Entity.dotted("com.example.BaseNode.foo"),
        Entity.dotted("com.example.BaseNode.bar"),
        Entity.dotted("com.example.BaseNode.e"),
        Entity.dotted("com.example.BaseNode.localV")
      ),
      SortedSet(
        Entity.dotted("???"),
        Entity.dotted("com"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.dogs"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.animal.dogs.retriever.Bar"),
        Entity.dotted("com.example.???"),
        Entity.dotted("com.animal.dogs.gamma"),
        Entity.dotted("com.animal.dogs.gamma.Square"),
        Entity.dotted("Data"),
        Entity.dotted("Option"),
        Entity.dotted("TypeB"),
        Entity.dotted("TypeC"),
        Entity.dotted("baz"),
        Entity.dotted("com.example.Data"),
        Entity.dotted("com.example.Option"),
        Entity.dotted("com.example.TypeB"),
        Entity.dotted("com.example.TypeC"),
        Entity.dotted("com.example.baz"),
        Entity.dotted("com.example.z"),
        Entity.dotted("com.example.z.TypeA"),
        Entity.dotted("z"),
        Entity.dotted("z.TypeA"),
        Entity.dotted("TypeE"),
        Entity.dotted("TypeF"),
        Entity.dotted("com.example.TypeE"),
        Entity.dotted("com.example.TypeF"),
        Entity.dotted("com.example.z.TypeG"),
        Entity.dotted("z.TypeG"),
        Entity.dotted("z.TypeH"),
        Entity.dotted("com.example.z.TypeH"),
        Entity.dotted("TRRF"),
        Entity.dotted("TRRF.config"),
        Entity.dotted("com.example.TRRF"),
        Entity.dotted("com.example.TRRF.config"),
        Entity.dotted("Encoder"),
        Entity.dotted("FeC"),
        Entity.dotted("TRRF.config.withId"),
        Entity.dotted("Zed"),
        Entity.dotted("Zed.K"),
        Entity.dotted("com.example.Encoder"),
        Entity.dotted("com.example.FeC"),
        Entity.dotted("com.example.TRRF.config.withId"),
        Entity.dotted("com.example.Zed"),
        Entity.dotted("com.example.Zed.K"),
        Entity.dotted("com.example.z.TypeJ"),
        Entity.dotted("z.TypeJ"),
        Entity.dotted("Some"),
        Entity.dotted("com.example.Some"),
        Entity.dotted("com.example.durationWindow"),
        Entity.dotted("com.example.playDurationWindow"),
        Entity.dotted("com.example.z.TypeK"),
        Entity.dotted("com.example.z.TypeL"),
        Entity.dotted("durationWindow"),
        Entity.dotted("playDurationWindow"),
        Entity.dotted("z.TypeK"),
        Entity.dotted("z.TypeL")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Failing sample") {
    val simpleContent = """
package com.example

object syntax
    extends types.A.Ops
    with types.B.Ops
    with E.C.Ops
    with E.G.Ops
    with H.L.Ops
    with P.Q.Ops
"""
    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.example.syntax")
      ),
      SortedSet(
        Entity.dotted("E"),
        Entity.dotted("E.C"),
        Entity.dotted("E.G"),
        Entity.dotted("H"),
        Entity.dotted("H.L"),
        Entity.dotted("Ops"),
        Entity.dotted("P"),
        Entity.dotted("P.Q"),
        Entity.dotted("com.example.E"),
        Entity.dotted("com.example.E.C"),
        Entity.dotted("com.example.E.G"),
        Entity.dotted("com.example.H"),
        Entity.dotted("com.example.H.L"),
        Entity.dotted("com.example.Ops"),
        Entity.dotted("com.example.P"),
        Entity.dotted("com.example.P.Q"),
        Entity.dotted("com.example.types"),
        Entity.dotted("com.example.types.A"),
        Entity.dotted("com.example.types.B"),
        Entity.dotted("types"),
        Entity.dotted("types.A"),
        Entity.dotted("types.B"),
        Entity.dotted("E.C.Ops"),
        Entity.dotted("E.G.Ops"),
        Entity.dotted("H.L.Ops"),
        Entity.dotted("P.Q.Ops"),
        Entity.dotted("com.example.E.C.Ops"),
        Entity.dotted("com.example.E.G.Ops"),
        Entity.dotted("com.example.H.L.Ops"),
        Entity.dotted("com.example.P.Q.Ops"),
        Entity.dotted("com.example.types.A.Ops"),
        Entity.dotted("com.example.types.B.Ops"),
        Entity.dotted("types.A.Ops"),
        Entity.dotted("types.B.Ops")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("object value") {
    val simpleContent = """
package com.example
import z.TypeA

object MyObject {
    @Option(foo,bar,baz)
    val foo: Data[TypeA, TypeB[TypeC]] = "asdf"
}

"""
    val expectedSymbols = Symbols(
      SortedSet(
        Entity.dotted("com.example.MyObject"),
        Entity.dotted("com.example.MyObject.foo")
      ),
      SortedSet(
        Entity.dotted("Data"),
        Entity.dotted("TypeB"),
        Entity.dotted("TypeC"),
        Entity.dotted("com.example.Data"),
        Entity.dotted("com.example.TypeB"),
        Entity.dotted("com.example.TypeC"),
        Entity.dotted("com.example.Option"),
        Entity.dotted("Option"),
        Entity.dotted("com.example.bar"),
        Entity.dotted("bar"),
        Entity.dotted("baz"),
        Entity.dotted("com.example.baz"),
        Entity.dotted("com.example.z"),
        Entity.dotted("com.example.z.TypeA"),
        Entity.dotted("z"),
        Entity.dotted("z.TypeA")
      ),
      SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Wildcard import") {
    // Its hard to know if a wildcard implicit is being used somewhere in the code, so we have to link
    // to the wildcard :/
    val simpleContent = """
        package com.foo.bar

        import com.baz.buzz.Dope._
        case class Cat(foo: String) {
            def bar(implicit np: Nope): Long = ???
        }
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.bar"),
        Entity.dotted("com.foo.bar.Cat.foo")
      ),
      refs = SortedSet(
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.String"),
        Entity.dotted("???"),
        Entity.dotted("Long"),
        Entity.dotted("Nope"),
        Entity.dotted("com"),
        Entity.dotted("com.baz"),
        Entity.dotted("com.baz.buzz"),
        Entity.dotted("com.baz.buzz.Dope"),
        Entity.dotted("com.baz.buzz.Dope.???"),
        Entity.dotted("com.baz.buzz.Dope.Long"),
        Entity.dotted("com.baz.buzz.Dope.Nope"),
        Entity.dotted("com.baz.buzz.Dope.String"),
        Entity.dotted("com.foo.bar.???"),
        Entity.dotted("com.foo.bar.Long"),
        Entity.dotted("com.foo.bar.Nope")
      ),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Refer to object defined elsewhere") {
    val simpleContent = """
        package com.foo.bar
        import com.monovore.decline.{ Command, Opts }
        object App {
            private val command = Command("myfoo", "jar to containing classes tool") {
            }
        }
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.App"),
        Entity.dotted("com.foo.bar.App.command")
      ),
      refs = SortedSet(
        Entity.dotted("com"),
        Entity.dotted("com.monovore"),
        Entity.dotted("com.monovore.decline"),
        Entity.dotted("com.monovore.decline.Command"),
        Entity.dotted("com.monovore.decline.Opts")
      ),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

  test("Failing refs case") {
    val simpleContent = """
        package com.foo.bar
        object App {
            protected case class Val(name: String) extends super.Val
            def foo() {
                val ccc = new com.animal.foo.bar.baz.CustTpe(sparkSession)
            }
        }
    """
    val expectedSymbols = Symbols(
      defs = SortedSet(
        Entity.dotted("com.foo.bar.App"),
        Entity.dotted("com.foo.bar.App.foo"),
        Entity.dotted("com.foo.bar.App.Val"),
        Entity.dotted("com.foo.bar.App.Val.name")
      ),
      refs = SortedSet(
        Entity.dotted("com"),
        Entity.dotted("CustTpe"),
        Entity.dotted("Unit"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.foo"),
        Entity.dotted("com.animal.foo.bar"),
        Entity.dotted("com.animal.foo.bar.baz"),
        Entity.dotted("com.foo.bar.CustTpe"),
        Entity.dotted("com.foo.bar.Unit"),
        Entity.dotted("com.foo.bar.sparkSession"),
        Entity.dotted("sparkSession"),
        Entity.dotted("com.animal.foo.bar.baz.CustTpe"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.String")
      ),
      bzl_gen_build_commands = SortedSet.empty
    )
    assertParse(simpleContent, expectedSymbols, List("com"))
  }

}
