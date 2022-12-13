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
      bzl_gen_build_commands = SortedSet()
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("can extract a strange syntax") {
    val simpleContent = """
    package com.foo.bar

    case class Cat(foo: String) {
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
        Entity.dotted("com.foo.bar.String")
      ),
      SortedSet()
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
      SortedSet()
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("can extract a failing file 2") {
    val simpleContent = """
package com.foo.bar
trait TestTrait {
  def foo(): Long = {
      combinedParams.underlying.collect {
        case (param: OtherTrait[param], value: String) if outCols.contains(param) => (param, value)
      }
  }
}
    """
    val expectedDataBlock = DataBlock(
      "",
      SortedSet(
        Entity.dotted("com.foo.bar.TestTrait"),
        Entity.dotted("com.foo.bar.TestTrait.foo")
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
        Entity.dotted("value")
      ),
      SortedSet()
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
        "link: com.example.BaseNode -> com.example.com.animal.cats.tiger.TigerStripes"
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
        "link: com.example.BaseNode -> com.example.com.animal.cats.tiger.TigerStripes"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

  test("Add public method link") {
    val simpleContent = """
package com.example
import com.animal.dogs.retriever.Bar
import com.animal.dogs.gamma.Square

case class BaseNode() {
    def myFunction(square: Square): Bar = {
        ???
    }
}
"""
    val expectedDataBlock = DataBlock(
      "",
      SortedSet(
        Entity.dotted("com.example.BaseNode"),
        Entity.dotted("com.example.BaseNode.myFunction")
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
        Entity.dotted("com.example.com.animal.dogs.gamma.Square")
      ),
      SortedSet(
        "link: com.example.BaseNode -> com.animal.dogs.retriever.Bar",
        "link: com.example.BaseNode -> com.example.com.animal.dogs.retriever.Bar",
        "link: com.example.BaseNode -> com.animal.dogs.gamma.Square",
        "link: com.example.BaseNode -> com.example.com.animal.dogs.gamma.Square"
      )
    )
    assertParse(simpleContent, expectedDataBlock)
  }

}
