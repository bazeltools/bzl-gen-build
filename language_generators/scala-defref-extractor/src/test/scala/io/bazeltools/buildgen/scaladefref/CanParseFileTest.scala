package io.bazeltools.buildgen.scaladefref

import org.scalatest.funsuite.AnyFunSuite
import io.bazeltools.buildgen.shared._
import scala.collection.immutable.SortedSet
import cats.effect.unsafe.implicits.global

class CanParseFileTest extends AnyFunSuite {

  def assertParse(str: String, expected: DataBlock) =
    assert(ScalaSourceEntityExtractor.extract(str).unsafeRunSync() === expected)

  test("can extract a simple file") {
    val simpleContent = """
    package com.foo.bar

    case class Cat(foo: String)
    """
    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.foo")
      ),
      refs =
        SortedSet(Entity.dotted("String"), Entity.dotted("com.foo.bar.String")),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.Cat -> com.foo.bar.String",
        "link: com.foo.bar.Cat -> String",
        "link: com.foo.bar.Cat -> com.foo.bar.String"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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

    val expectedDataBlock = DataBlock(
      "",
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
      SortedSet(
        "link: com.foo.bar.Cat -> com.example.Elephant",
        "link: com.foo.bar.Cat -> com.example.Wolf",
        "link: com.foo.bar.Cat -> com.foo.bar.com.example.Elephant",
        "link: com.foo.bar.Cat -> com.foo.bar.com.example.Wolf",
        "link: com.foo.bar.Cat -> Dog",
        "link: com.foo.bar.Cat -> com.foo.bar.Dog",
        "link: com.foo.bar.Cat -> String",
        "link: com.foo.bar.Cat -> com.foo.bar.String"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("can extract a failing file") {
    val simpleContent = """
    package com.foo.bar

    object TestObj {
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
    val expectedDataBlock = DataBlock(
      "",
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
        Entity.dotted("println")
      ),
      SortedSet(
        "link: com.foo.bar.TestObj -> TensorDataType",
        "link: com.foo.bar.TestObj -> com.foo.bar.TensorDataType",
        "link: com.foo.bar.TestObj -> com.foo.bar.forAll",
        "link: com.foo.bar.TestObj -> com.foo.bar.genSparseTensorOf",
        "link: com.foo.bar.TestObj -> forAll",
        "link: com.foo.bar.TestObj -> genSparseTensorOf"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
      SortedSet(
        "link: com.foo.bar.TestTrait -> Long",
        "link: com.foo.bar.TestTrait -> com.foo.bar.Long",
        "link: com.foo.bar.TestTrait -> com.foo.bar.z.GenA",
        "link: com.foo.bar.TestTrait -> com.foo.bar.z.GenB",
        "link: com.foo.bar.TestTrait -> z.GenA",
        "link: com.foo.bar.TestTrait -> z.GenB",
        "link: com.foo.bar.TestTrait -> com.foo.bar.Zeb",
        "link: com.foo.bar.Zeb -> com.foo.bar.Zeb.np",
        "link: com.foo.bar.Zeb -> com.foo.bar.z.NP",
        "link: com.foo.bar.Zeb -> z.NP",
        "link: com.foo.bar.TestTrait -> com.foo.bar.org.apache.spark.sql.functions.udf",
        "link: com.foo.bar.TestTrait -> org.apache.spark.sql.functions.udf",
        "link: com.foo.bar.TestTrait -> Dependency",
        "link: com.foo.bar.TestTrait -> com.foo.bar.Dependency",
        "link: com.foo.bar.TestTrait -> com.foo.bar.TestTrait.foo",
        "link: com.foo.bar.TestTrait -> com.foo.bar.getCountryCode",
        "link: com.foo.bar.TestTrait -> com.foo.bar.z.OT",
        "link: com.foo.bar.TestTrait -> getCountryCode",
        "link: com.foo.bar.TestTrait -> z.OT",
        "link: com.foo.bar.TestTrait -> OtherClass",
        "link: com.foo.bar.TestTrait -> OtherClass.OtherSubClass",
        "link: com.foo.bar.TestTrait -> OtherSubClass",
        "link: com.foo.bar.TestTrait -> com.foo.bar.OtherClass",
        "link: com.foo.bar.TestTrait -> com.foo.bar.OtherClass.OtherSubClass",
        "link: com.foo.bar.TestTrait -> com.foo.bar.OtherSubClass"
      )
    )

    assertParse(simpleContent, expectedDataBlock)
  }

  test("Add transitive links") {
    val simpleContent = """
        package com.foo.bar

        case class Cat(foo: String) extends Dog
    """
    val expectedDataBlock = DataBlock(
      "",
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
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.Cat -> Dog",
        "link: com.foo.bar.Cat -> com.foo.bar.Dog",
        "link: com.foo.bar.Cat -> String",
        "link: com.foo.bar.Cat -> com.foo.bar.String"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("Class arg") {
    val simpleContent = """
        package com.foo.bar

        class Cat(dog: Dog) {
        }
    """
    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(
        Entity.dotted("com.foo.bar.Cat"),
        Entity.dotted("com.foo.bar.Cat.dog")
      ),
      refs = SortedSet(
        Entity.dotted("Dog"),
        Entity.dotted("com.foo.bar.Dog")
      ),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.Cat -> Dog",
        "link: com.foo.bar.Cat -> com.foo.bar.Dog"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("Add more transitive links") {
    val simpleContent = """
        package com.foo.bar

  sealed abstract class AbstractC
    extends BaseType[BaseTpeParamA, BaseTpeParamB, BaseTpeParamC]{}
    """
    val expectedDataBlock = DataBlock(
      "",
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
      SortedSet(
        "link: com.foo.bar.AbstractC -> BaseTpeParamA",
        "link: com.foo.bar.AbstractC -> BaseTpeParamB",
        "link: com.foo.bar.AbstractC -> BaseTpeParamC",
        "link: com.foo.bar.AbstractC -> BaseType",
        "link: com.foo.bar.AbstractC -> com.foo.bar.BaseTpeParamA",
        "link: com.foo.bar.AbstractC -> com.foo.bar.BaseTpeParamB",
        "link: com.foo.bar.AbstractC -> com.foo.bar.BaseTpeParamC",
        "link: com.foo.bar.AbstractC -> com.foo.bar.BaseType"
      )
    )

    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.retriever"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.animal.dogs.retriever.com"),
        Entity.dotted("com.animal.dogs.retriever.com.animal"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats.big"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs"),
        Entity.dotted("com.example.CaseClassConfig"),
        Entity.dotted("com.example.Express"),
        Entity.dotted("com.example.JsonEncoder"),
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
      SortedSet(
        "link: com.example.BaseNode -> CaseClassConfig",
        "link: com.example.BaseNode -> Express",
        "link: com.example.BaseNode -> JsonEncoder",
        "link: com.example.BaseNode -> com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.CaseClassConfig",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.Express",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.JsonEncoder",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.example.CaseClassConfig",
        "link: com.example.BaseNode -> com.example.Express",
        "link: com.example.BaseNode -> com.example.JsonEncoder",
        "link: com.example.BaseNode -> com.example.com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.example.com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.animal.dogs.retriever"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Cute"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs.Small"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.retriever"),
        Entity.dotted("com.animal.dogs.retriever"),
        Entity.dotted("com.animal.dogs.retriever.com"),
        Entity.dotted("com.animal.dogs.retriever.com.animal"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.cats.big"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs"),
        Entity.dotted("com.animal.dogs.retriever.com.animal.dogs.pugs"),
        Entity.dotted("com.example.CaseClassConfig"),
        Entity.dotted("com.example.Express"),
        Entity.dotted("com.example.JsonEncoder"),
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
      SortedSet(
        "link: com.example.BaseNode -> CaseClassConfig",
        "link: com.example.BaseNode -> Express",
        "link: com.example.BaseNode -> JsonEncoder",
        "link: com.example.BaseNode -> com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.CaseClassConfig",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.Express",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.JsonEncoder",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.example.CaseClassConfig",
        "link: com.example.BaseNode -> com.example.Express",
        "link: com.example.BaseNode -> com.example.JsonEncoder",
        "link: com.example.BaseNode -> com.example.com.animal.cats.big.BaseTrainingNode",
        "link: com.example.BaseNode -> com.example.com.animal.cats.tiger.TigerStripes",
        "link: com.example.BaseNode -> com.animal.dogs.retriever"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
        Entity.dotted("com.example.com"),
        Entity.dotted("com.example.com.animal"),
        Entity.dotted("com.example.com.animal.dogs"),
        Entity.dotted("com.example.com.animal.dogs.retriever"),
        Entity.dotted("com.example.com.animal.dogs.retriever.Bar"),
        Entity.dotted("com.animal.dogs.gamma"),
        Entity.dotted("com.animal.dogs.gamma.Square"),
        Entity.dotted("com.example.com.animal.dogs.gamma"),
        Entity.dotted("com.example.com.animal.dogs.gamma.Square"),
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
      SortedSet(
        "link: com.example.BaseNode -> Option",
        "link: com.example.BaseNode -> com.example.Option",
        "link: com.example.BaseNode -> com.example.TypeB",
        "link: com.example.BaseNode -> com.example.TypeC",
        "link: com.example.BaseNode -> com.example.z.TypeA",
        "link: com.example.BaseNode -> z.TypeA",
        "link: com.example.BaseNode -> Data",
        "link: com.example.BaseNode -> TypeB",
        "link: com.example.BaseNode -> TypeC",
        "link: com.example.BaseNode -> com.animal.dogs.gamma.Square",
        "link: com.example.BaseNode -> com.animal.dogs.retriever.Bar",
        "link: com.example.BaseNode -> com.example.Data",
        "link: com.example.BaseNode -> com.example.com.animal.dogs.gamma.Square",
        "link: com.example.BaseNode -> com.example.com.animal.dogs.retriever.Bar",
        "link: com.example.BaseNode -> TypeE",
        "link: com.example.BaseNode -> TypeF",
        "link: com.example.BaseNode -> com.example.TypeE",
        "link: com.example.BaseNode -> com.example.TypeF",
        "link: com.example.BaseNode -> com.example.z.TypeG",
        "link: com.example.BaseNode -> z.TypeG",
        "link: com.example.BaseNode -> z.TypeH",
        "link: com.example.BaseNode -> com.example.z.TypeH",
        "link: com.example.BaseNode -> TRRF",
        "link: com.example.BaseNode -> com.example.TRRF",
        "link: com.example.BaseNode -> Encoder",
        "link: com.example.BaseNode -> FeC",
        "link: com.example.BaseNode -> Zed",
        "link: com.example.BaseNode -> com.example.Encoder",
        "link: com.example.BaseNode -> com.example.FeC",
        "link: com.example.BaseNode -> com.example.Zed",
        "link: com.example.BaseNode -> com.example.z.TypeJ",
        "link: com.example.BaseNode -> z.TypeJ",
        "link: com.example.BaseNode -> Some",
        "link: com.example.BaseNode -> baz",
        "link: com.example.BaseNode -> com.example.Some",
        "link: com.example.BaseNode -> com.example.baz",
        "link: com.example.BaseNode -> com.example.durationWindow",
        "link: com.example.BaseNode -> com.example.z",
        "link: com.example.BaseNode -> com.example.z.TypeL",
        "link: com.example.BaseNode -> durationWindow",
        "link: com.example.BaseNode -> z",
        "link: com.example.BaseNode -> z.TypeL"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
      SortedSet(
        "link: com.example.syntax -> E",
        "link: com.example.syntax -> E.C",
        "link: com.example.syntax -> E.C.Ops",
        "link: com.example.syntax -> E.G",
        "link: com.example.syntax -> E.G.Ops",
        "link: com.example.syntax -> H",
        "link: com.example.syntax -> H.L",
        "link: com.example.syntax -> H.L.Ops",
        "link: com.example.syntax -> Ops",
        "link: com.example.syntax -> P",
        "link: com.example.syntax -> P.Q",
        "link: com.example.syntax -> P.Q.Ops",
        "link: com.example.syntax -> com.example.E",
        "link: com.example.syntax -> com.example.E.C",
        "link: com.example.syntax -> com.example.E.C.Ops",
        "link: com.example.syntax -> com.example.E.G",
        "link: com.example.syntax -> com.example.E.G.Ops",
        "link: com.example.syntax -> com.example.H",
        "link: com.example.syntax -> com.example.H.L",
        "link: com.example.syntax -> com.example.H.L.Ops",
        "link: com.example.syntax -> com.example.Ops",
        "link: com.example.syntax -> com.example.P",
        "link: com.example.syntax -> com.example.P.Q",
        "link: com.example.syntax -> com.example.P.Q.Ops",
        "link: com.example.syntax -> com.example.types",
        "link: com.example.syntax -> com.example.types.A",
        "link: com.example.syntax -> com.example.types.A.Ops",
        "link: com.example.syntax -> com.example.types.B",
        "link: com.example.syntax -> com.example.types.B.Ops",
        "link: com.example.syntax -> types",
        "link: com.example.syntax -> types.A",
        "link: com.example.syntax -> types.A.Ops",
        "link: com.example.syntax -> types.B",
        "link: com.example.syntax -> types.B.Ops"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
      SortedSet(
        "link: com.example.MyObject -> Data",
        "link: com.example.MyObject -> TypeB",
        "link: com.example.MyObject -> TypeC",
        "link: com.example.MyObject -> com.example.Data",
        "link: com.example.MyObject -> com.example.TypeB",
        "link: com.example.MyObject -> com.example.TypeC",
        "link: com.example.MyObject -> Option",
        "link: com.example.MyObject -> com.example.Option",
        "link: com.example.MyObject -> com.example.z.TypeA",
        "link: com.example.MyObject -> z.TypeA"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
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
        Entity.dotted("com.baz.buzz.Dope.com"),
        Entity.dotted("com.baz.buzz.Dope.com.baz"),
        Entity.dotted("com.baz.buzz.Dope.com.baz.buzz"),
        Entity.dotted("com.baz.buzz.Dope.com.baz.buzz.Dope"),
        Entity.dotted("com.foo.bar.???"),
        Entity.dotted("com.foo.bar.Long"),
        Entity.dotted("com.foo.bar.Nope"),
        Entity.dotted("com.foo.bar.com"),
        Entity.dotted("com.foo.bar.com.baz"),
        Entity.dotted("com.foo.bar.com.baz.buzz"),
        Entity.dotted("com.foo.bar.com.baz.buzz.Dope")
      ),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.Cat -> Long",
        "link: com.foo.bar.Cat -> Nope",
        "link: com.foo.bar.Cat -> com.baz.buzz.Dope",
        "link: com.foo.bar.Cat -> com.baz.buzz.Dope.Long",
        "link: com.foo.bar.Cat -> com.baz.buzz.Dope.Nope",
        "link: com.foo.bar.Cat -> com.foo.bar.Long",
        "link: com.foo.bar.Cat -> com.foo.bar.Nope",
        "link: com.foo.bar.Cat -> String",
        "link: com.foo.bar.Cat -> com.baz.buzz.Dope.String",
        "link: com.foo.bar.Cat -> com.foo.bar.String"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(
        Entity.dotted("com.foo.bar.App"),
        Entity.dotted("com.foo.bar.App.command")
      ),
      refs = SortedSet(
        Entity.dotted("com"),
        Entity.dotted("com.foo.bar.com"),
        Entity.dotted("com.foo.bar.com.monovore"),
        Entity.dotted("com.foo.bar.com.monovore.decline"),
        Entity.dotted("com.foo.bar.com.monovore.decline.Command"),
        Entity.dotted("com.foo.bar.com.monovore.decline.Opts"),
        Entity.dotted("com.monovore"),
        Entity.dotted("com.monovore.decline"),
        Entity.dotted("com.monovore.decline.Command"),
        Entity.dotted("com.monovore.decline.Opts")
      ),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.App -> com.foo.bar.com.monovore.decline.Command",
        "link: com.foo.bar.App -> com.monovore.decline.Command"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
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
    val expectedDataBlock = DataBlock(
      "",
      defs = SortedSet(
        Entity.dotted("com.foo.bar.App"),
        Entity.dotted("com.foo.bar.App.foo"),
        Entity.dotted("com.foo.bar.App.Val"),
        Entity.dotted("com.foo.bar.App.Val.name")
      ),
      refs = SortedSet(
        Entity.dotted("com"),
        Entity.dotted("com.foo.bar.com"),
        Entity.dotted("CustTpe"),
        Entity.dotted("Unit"),
        Entity.dotted("com.animal"),
        Entity.dotted("com.animal.foo"),
        Entity.dotted("com.animal.foo.bar"),
        Entity.dotted("com.animal.foo.bar.baz"),
        Entity.dotted("com.foo.bar.CustTpe"),
        Entity.dotted("com.foo.bar.Unit"),
        Entity.dotted("com.foo.bar.com.animal"),
        Entity.dotted("com.foo.bar.com.animal.foo"),
        Entity.dotted("com.foo.bar.com.animal.foo.bar"),
        Entity.dotted("com.foo.bar.com.animal.foo.bar.baz"),
        Entity.dotted("com.foo.bar.sparkSession"),
        Entity.dotted("sparkSession"),
        Entity.dotted("com.animal.foo.bar.baz.CustTpe"),
        Entity.dotted("com.foo.bar.com.animal.foo.bar.baz.CustTpe"),
        Entity.dotted("String"),
        Entity.dotted("com.foo.bar.String")
      ),
      bzl_gen_build_commands = SortedSet(
        "link: com.foo.bar.App -> Unit",
        "link: com.foo.bar.App -> com.foo.bar.Unit",
        "link: com.foo.bar.App.Val -> String",
        "link: com.foo.bar.App.Val -> com.foo.bar.String"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

}
